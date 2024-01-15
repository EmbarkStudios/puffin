use crate::{
    NanoSecond, Reader, Result, Scope, ScopeCollection, ScopeId, Stream, ThreadInfo,
    UnpackedFrameData,
};
use std::{collections::BTreeMap, hash::Hash};

/// Temporary structure while building a [`MergeScope`].
#[derive(Clone, Debug, PartialEq, PartialOrd, Eq, Ord, Hash)]
struct MergeId<'s> {
    id: ScopeId,
    data: &'s str,
}

/// Temporary structure while building a [`MergeScope`].
#[derive(Default)]
struct MergeNode<'s> {
    /// These are the raw scopes that got merged into us.
    /// All these scopes have the same `id`.
    pieces: Vec<MergePiece<'s>>,

    /// indexed by their id+data
    children: BTreeMap<MergeId<'s>, MergeNode<'s>>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct MergePiece<'s> {
    /// The start of the scope relative to its *parent* [`Scope`].
    pub relative_start_ns: NanoSecond,
    /// The raw scope, just like it is found in the input stream
    pub scope: Scope<'s>,
}

/// A scope that has been merged from many different sources
#[derive(Clone, Debug, PartialEq)]
pub struct MergeScope<'s> {
    /// Relative to parent.
    pub relative_start_ns: NanoSecond,
    /// Sum of all durations over all frames
    pub total_duration_ns: NanoSecond,
    /// [`Self::total_duration_ns`] divided by number of frames.
    pub duration_per_frame_ns: NanoSecond,
    /// The slowest individual piece.
    pub max_duration_ns: NanoSecond,
    /// Number of pieces that got merged together to us.
    pub num_pieces: usize,
    /// The common identifier that we merged using.
    pub id: ScopeId,
    /// only set if all children had the same
    pub data: std::borrow::Cow<'s, str>,
    /// The merged children of this merged scope.
    pub children: Vec<MergeScope<'s>>,
}

impl<'s> MergeScope<'s> {
    /// Clones the merge scope.
    pub fn into_owned(self) -> MergeScope<'static> {
        MergeScope::<'static> {
            relative_start_ns: self.relative_start_ns,
            total_duration_ns: self.total_duration_ns,
            duration_per_frame_ns: self.duration_per_frame_ns,
            max_duration_ns: self.max_duration_ns,
            num_pieces: self.num_pieces,
            id: self.id,
            data: std::borrow::Cow::Owned(self.data.into_owned()),
            children: self.children.into_iter().map(Self::into_owned).collect(),
        }
    }
}

impl<'s> MergeNode<'s> {
    fn add<'slf>(&'slf mut self, stream: &'s Stream, piece: MergePiece<'s>) -> Result<()> {
        self.pieces.push(piece);

        for child in Reader::with_offset(stream, piece.scope.child_begin_position)? {
            let child = child?;

            self.children
                .entry(MergeId {
                    id: child.id,
                    data: child.record.data,
                })
                .or_default()
                .add(
                    stream,
                    MergePiece {
                        relative_start_ns: child.record.start_ns - piece.scope.record.start_ns,
                        scope: child,
                    },
                )?;
        }

        Ok(())
    }

    fn build(self, scope_collection: &ScopeCollection, num_frames: i64) -> MergeScope<'s> {
        assert!(!self.pieces.is_empty());
        let mut relative_start_ns = self.pieces[0].relative_start_ns;
        let mut total_duration_ns = 0;
        let mut slowest_ns = 0;
        let num_pieces = self.pieces.len();
        let id = self.pieces[0].scope.id;
        let mut data = self.pieces[0].scope.record.data;

        for piece in &self.pieces {
            // Merged scope should start at the earliest piece:
            relative_start_ns = relative_start_ns.min(piece.relative_start_ns);
            total_duration_ns += piece.scope.record.duration_ns;
            slowest_ns = slowest_ns.max(piece.scope.record.duration_ns);

            assert_eq!(id, piece.scope.id);
            if data != piece.scope.record.data {
                data = ""; // different in different pieces
            }
        }

        MergeScope {
            relative_start_ns,
            total_duration_ns,
            duration_per_frame_ns: total_duration_ns / num_frames,
            max_duration_ns: slowest_ns,
            num_pieces,
            id,
            data: data.into(),
            children: build(scope_collection, self.children, num_frames),
        }
    }
}

fn build<'s>(
    scope_collection: &ScopeCollection,
    nodes: BTreeMap<MergeId<'s>, MergeNode<'s>>,
    num_frames: i64,
) -> Vec<MergeScope<'s>> {
    let mut scopes: Vec<_> = nodes
        .into_values()
        .map(|node| node.build(scope_collection, num_frames))
        .collect();

    // Earliest first:
    scopes.sort_by_key(|scope| scope.relative_start_ns);

    // Make sure sibling scopes do not overlap:
    let mut relative_ns = 0;
    for scope in &mut scopes {
        scope.relative_start_ns = scope.relative_start_ns.max(relative_ns);
        relative_ns = scope.relative_start_ns + scope.duration_per_frame_ns;
    }

    scopes
}

/// For the given thread, merge all scopes with the same id+data path.
pub fn merge_scopes_for_thread<'s>(
    scope_collection: &ScopeCollection,
    frames: &'s [std::sync::Arc<UnpackedFrameData>],
    thread_info: &ThreadInfo,
) -> Result<Vec<MergeScope<'s>>> {
    let mut top_nodes: BTreeMap<MergeId<'s>, MergeNode<'s>> = Default::default();

    for frame in frames {
        if let Some(stream_info) = frame.thread_streams.get(thread_info) {
            let offset_ns = frame.meta.range_ns.0 - frames[0].meta.range_ns.0; // make everything relative to first frame

            let top_scopes = Reader::from_start(&stream_info.stream).read_top_scopes()?;
            for scope in top_scopes {
                top_nodes
                    .entry(MergeId {
                        id: scope.id,
                        data: scope.record.data,
                    })
                    .or_default()
                    .add(
                        &stream_info.stream,
                        MergePiece {
                            relative_start_ns: scope.record.start_ns - offset_ns,
                            scope,
                        },
                    )?;
            }
        }
    }

    Ok(build(scope_collection, top_nodes, frames.len() as _))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, sync::Arc};

    #[test]
    fn test_merge() {
        use crate::*;

        let mut scope_collection = ScopeCollection::default();
        // top scopes
        scope_collection.insert(Arc::new(
            ScopeDetails::from_scope_id(ScopeId::new(1)).with_function_name("a"),
        ));
        scope_collection.insert(Arc::new(
            ScopeDetails::from_scope_id(ScopeId::new(2)).with_function_name("b"),
        ));

        // middle scopes
        scope_collection.insert(Arc::new(
            ScopeDetails::from_scope_id(ScopeId::new(3)).with_function_name("ba"),
        ));
        scope_collection.insert(Arc::new(
            ScopeDetails::from_scope_id(ScopeId::new(4)).with_function_name("bb"),
        ));
        scope_collection.insert(Arc::new(
            ScopeDetails::from_scope_id(ScopeId::new(5)).with_function_name("bba"),
        ));

        let stream = {
            let mut stream = Stream::default();

            for i in 0..2 {
                let ns = 1000 * i;

                let (a, _) = stream.begin_scope(|| ns + 100, ScopeId::new(1), "");
                stream.end_scope(a, ns + 200);

                let (b, _) = stream.begin_scope(|| ns + 200, ScopeId::new(2), "");

                let (ba, _) = stream.begin_scope(|| ns + 400, ScopeId::new(3), "");
                stream.end_scope(ba, ns + 600);

                let (bb, _) = stream.begin_scope(|| ns + 600, ScopeId::new(4), "");
                let (bba, _) = stream.begin_scope(|| ns + 600, ScopeId::new(5), "");
                stream.end_scope(bba, ns + 700);
                stream.end_scope(bb, ns + 800);
                stream.end_scope(b, ns + 900);
            }

            stream
        };

        let stream_info = StreamInfo::parse(stream).unwrap();
        let mut thread_streams = BTreeMap::new();
        let thread_info = ThreadInfo {
            start_time_ns: Some(0),
            name: "main".to_owned(),
        };
        thread_streams.insert(thread_info.clone(), stream_info);
        let frame = UnpackedFrameData::new(0, thread_streams).unwrap();
        let frames = [Arc::new(frame)];
        let merged = merge_scopes_for_thread(&scope_collection, &frames, &thread_info).unwrap();

        let expected = vec![
            MergeScope {
                relative_start_ns: 100,
                total_duration_ns: 2 * 100,
                duration_per_frame_ns: 2 * 100,
                max_duration_ns: 100,
                num_pieces: 2,
                id: ScopeId::new(1),
                data: "".into(),
                children: vec![],
            },
            MergeScope {
                relative_start_ns: 300, // moved forward to make place for "a" (as are all children)
                total_duration_ns: 2 * 700,
                duration_per_frame_ns: 2 * 700,
                max_duration_ns: 700,
                num_pieces: 2,
                id: ScopeId::new(2),
                data: "".into(),
                children: vec![
                    MergeScope {
                        relative_start_ns: 200,
                        total_duration_ns: 2 * 200,
                        duration_per_frame_ns: 2 * 200,
                        max_duration_ns: 200,
                        num_pieces: 2,
                        id: ScopeId::new(3),
                        data: "".into(),
                        children: vec![],
                    },
                    MergeScope {
                        relative_start_ns: 600,
                        total_duration_ns: 2 * 200,
                        duration_per_frame_ns: 2 * 200,
                        max_duration_ns: 200,
                        num_pieces: 2,
                        id: ScopeId::new(4),
                        data: "".into(),
                        children: vec![MergeScope {
                            relative_start_ns: 0,
                            total_duration_ns: 2 * 100,
                            duration_per_frame_ns: 2 * 100,
                            max_duration_ns: 100,
                            num_pieces: 2,
                            id: ScopeId::new(5),
                            data: "".into(),
                            children: vec![],
                        }],
                    },
                ],
            },
        ];

        assert_eq!(
            merged, expected,
            "\nGot:\n{merged:#?}\n\n!=\nExpected:\n{expected:#?}",
        );
    }
}
