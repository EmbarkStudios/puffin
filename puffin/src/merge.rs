use crate::{NanoSecond, Reader, Result, Scope, Stream};
use std::collections::HashMap;

/// Temporary structure while building a `MergeScope`.
#[derive(Default)]
struct MergeNode<'s> {
    /// These are the raw scopes that got merged into us.
    /// All these scopes have the same `id`.
    pieces: Vec<MergePiece<'s>>,

    /// indexed by their id
    children: HashMap<&'s str, MergeNode<'s>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MergePiece<'s> {
    /// The start of the scope relative to its *parent* [`Scope`] (not to any [`MergeScope`]).
    pub relative_start_ns: NanoSecond,
    /// The raw scope, just like it is found in the input stream
    pub scope: Scope<'s>,
}

/// A scope that has been merged from many different sources
#[derive(Clone, Debug, PartialEq)]
pub struct MergeScope<'s> {
    /// Relative to parent
    pub relative_start_ns: NanoSecond,
    /// sum of all scopes that got merged into us.
    pub total_duration_ns: NanoSecond,
    /// The slowest individual piece.
    pub max_duration_ns: NanoSecond,
    /// Number of pieces that got merged together to us.
    pub num_pieces: usize,
    /// The common identifier that we merged using.
    pub id: &'s str,
    /// only set if all children had the same
    pub location: &'s str,
    /// only set if all children had the same
    pub data: &'s str,

    pub children: Vec<MergeScope<'s>>,
}

impl<'s> MergeNode<'s> {
    fn add<'slf>(&'slf mut self, stream: &'s Stream, piece: MergePiece<'s>) -> Result<()> {
        self.pieces.push(piece);

        for child in Reader::with_offset(stream, piece.scope.child_begin_position)? {
            let child = child?;
            self.children.entry(child.record.id).or_default().add(
                stream,
                MergePiece {
                    relative_start_ns: child.record.start_ns - piece.scope.record.start_ns,
                    scope: child,
                },
            )?;
        }

        Ok(())
    }

    fn build(self) -> MergeScope<'s> {
        assert!(!self.pieces.is_empty());
        let mut relative_start_ns = self.pieces[0].relative_start_ns;
        let mut total_duration_ns = 0;
        let mut slowest_ns = 0;
        let num_pieces = self.pieces.len();
        let id = self.pieces[0].scope.record.id;
        let mut location = self.pieces[0].scope.record.location;
        let mut data = self.pieces[0].scope.record.data;

        for piece in &self.pieces {
            // Merged scope should start at the earliest piece:
            relative_start_ns = relative_start_ns.min(piece.relative_start_ns);
            total_duration_ns += piece.scope.record.duration_ns;
            slowest_ns = slowest_ns.max(piece.scope.record.duration_ns);

            assert_eq!(id, piece.scope.record.id);
            if data != piece.scope.record.data {
                data = ""; // different in different pieces
            }
            if location != piece.scope.record.location {
                location = ""; // different in different pieces
            }
        }

        MergeScope {
            relative_start_ns,
            total_duration_ns,
            max_duration_ns: slowest_ns,
            num_pieces,
            id,
            location,
            data,
            children: build(self.children),
        }
    }
}

fn build<'s>(mut nodes: HashMap<&'s str, MergeNode<'s>>) -> Vec<MergeScope<'s>> {
    let mut scopes: Vec<_> = nodes.drain().map(|(_, node)| node.build()).collect();

    // Earliest first:
    scopes.sort_by_key(|scope| scope.relative_start_ns);

    // Make sure sibling scopes do not overlap:
    let mut relative_ns = 0;
    for scope in &mut scopes {
        scope.relative_start_ns = scope.relative_start_ns.max(relative_ns);
        relative_ns = scope.relative_start_ns + scope.total_duration_ns;
    }

    scopes
}

/// Merge all scopes with the same id path in one or more streams (frames).
pub fn merge_scopes_in_streams<'s>(
    streams: impl Iterator<Item = &'s Stream>,
) -> Result<Vec<MergeScope<'s>>> {
    let mut top_nodes: HashMap<&'s str, MergeNode<'s>> = Default::default();

    for stream in streams {
        let top_scopes = Reader::from_start(stream).read_top_scopes()?;
        for scope in top_scopes {
            top_nodes.entry(scope.record.id).or_default().add(
                stream,
                MergePiece {
                    relative_start_ns: scope.record.start_ns,
                    scope,
                },
            )?;
        }
    }

    Ok(build(top_nodes))
}

// ----------------------------------------------------------------------------

#[test]
fn test_merge() {
    use crate::*;

    let stream = {
        let mut stream = Stream::default();

        for i in 0..2 {
            let ns = 1000 * i;
            let a = stream.begin_scope(ns + 100, "a", "", "");
            stream.end_scope(a, ns + 200);

            let b = stream.begin_scope(ns + 200, "b", "", "");

            let ba = stream.begin_scope(ns + 400, "ba", "", "");
            stream.end_scope(ba, ns + 600);

            let bb = stream.begin_scope(ns + 600, "bb", "", "");
            let bba = stream.begin_scope(ns + 600, "bba", "", "");
            stream.end_scope(bba, ns + 700);
            stream.end_scope(bb, ns + 800);
            stream.end_scope(b, ns + 900);
        }

        stream
    };
    let streams = vec![stream];
    let merged = merge_scopes_in_streams(streams.iter()).unwrap();

    let expected = vec![
        MergeScope {
            relative_start_ns: 100,
            total_duration_ns: 2 * 100,
            max_duration_ns: 100,
            num_pieces: 2,
            id: "a",
            location: "",
            data: "",
            children: vec![],
        },
        MergeScope {
            relative_start_ns: 300, // moved forward to make place for "a" (as are all children)
            total_duration_ns: 2 * 700,
            max_duration_ns: 700,
            num_pieces: 2,
            id: "b",
            location: "",
            data: "",
            children: vec![
                MergeScope {
                    relative_start_ns: 200,
                    total_duration_ns: 2 * 200,
                    max_duration_ns: 200,
                    num_pieces: 2,
                    id: "ba",
                    location: "",
                    data: "",
                    children: vec![],
                },
                MergeScope {
                    relative_start_ns: 600,
                    total_duration_ns: 2 * 200,
                    max_duration_ns: 200,
                    num_pieces: 2,
                    id: "bb",
                    location: "",
                    data: "",
                    children: vec![MergeScope {
                        relative_start_ns: 0,
                        total_duration_ns: 2 * 100,
                        max_duration_ns: 100,
                        num_pieces: 2,
                        id: "bba",
                        location: "",
                        data: "",
                        children: vec![],
                    }],
                },
            ],
        },
    ];

    assert_eq!(
        merged, expected,
        "\nGot:\n{:#?}\n\n!=\nExpected:\n{:#?}",
        merged, expected
    );
}
