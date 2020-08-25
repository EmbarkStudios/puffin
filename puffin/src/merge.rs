use crate::{NanoSecond, Reader, Record, Result, Scope, Stream};
use std::collections::HashMap;

/// An record of several sibling (or cousin) scopes.
///
/// For instance, if one scope has a hundred children of the same ID
/// the UI may want to merge them into one child before display.
#[derive(Clone, Debug, PartialEq)]
pub struct MergeScope<'s> {
    /// The aggregated information.
    ///
    /// `record.duration_ns` is the sum `self.pieces`.
    pub record: Record<'s>,

    /// These are the raw scopes that got merged into `self.record`.
    /// All these scopes have the same `id` is `self.record`.
    pub pieces: Vec<MergePiece<'s>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MergePiece<'s> {
    /// The start of the scope relative to its *parent* `Scope` (not to any `MergeScope`).
    pub relative_start_ns: NanoSecond,
    /// The raw scope, just like it is found in the input stream
    pub scope: Scope<'s>,
}

pub fn merge_top_scopes<'s>(scopes: &[Scope<'s>]) -> Vec<MergeScope<'s>> {
    merge_pieces(scopes.iter().map(|scope| MergePiece {
        relative_start_ns: scope.record.start_ns,
        scope: *scope,
    }))
}

pub fn merge_children_of_pieces<'s>(
    stream: &'s Stream,
    parent: &MergeScope<'s>,
) -> Result<Vec<MergeScope<'s>>> {
    // collect all children of all the pieces scopes:
    let mut child_pieces = Vec::new();
    for piece in &parent.pieces {
        for child in
            Reader::with_offset(stream, piece.scope.child_begin_position)?.read_top_scopes()?
        {
            child_pieces.push(MergePiece {
                relative_start_ns: child.record.start_ns - piece.scope.record.start_ns,
                scope: child,
            });
        }
    }

    let mut merges = merge_pieces(child_pieces);

    // Move from relative to absolute time:
    for merge in &mut merges {
        merge.record.start_ns += parent.record.start_ns;
    }

    Ok(merges)
}

/// Group scopes based on their `record.id` (deinterleaving).
/// The returned merge scopes uses relative times.
fn merge_pieces<'s>(pieces: impl IntoIterator<Item = MergePiece<'s>>) -> Vec<MergeScope<'s>> {
    let mut merges: Vec<MergeScope<'s>> = Default::default();
    let mut index_from_id: HashMap<&'s str, usize> = Default::default();

    for piece in pieces {
        let record = piece.scope.record;

        match index_from_id.get(record.id).cloned() {
            None => {
                index_from_id.insert(record.id, merges.len());
                merges.push(MergeScope {
                    record: Record {
                        start_ns: piece.relative_start_ns,
                        ..record
                    },
                    pieces: vec![piece],
                });
            }
            Some(index) => {
                let merge = &mut merges[index];

                // Merged scope should start at the earliest piece:
                merge.record.start_ns = merge.record.start_ns.min(piece.relative_start_ns);

                // Accumulate time:
                merge.record.duration_ns += record.duration_ns;

                if merge.record.data != record.data {
                    merge.record.data = ""; // different in different pieces
                }
                if merge.record.location != record.location {
                    merge.record.location = ""; // different in different pieces
                }

                merge.pieces.push(piece);
            }
        }
    }

    if !merges.is_empty() {
        // Earliest first:
        merges.sort_by_key(|merged_scope| merged_scope.record.start_ns);

        // Make sure children do not overlap:
        let mut ns = 0;
        for merge in &mut merges {
            merge.record.start_ns = merge.record.start_ns.max(ns);
            ns = merge.record.stop_ns();
        }
    }

    merges
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

    let top_scopes = Reader::from_start(&stream).read_top_scopes().unwrap();
    assert_eq!(top_scopes.len(), 4);

    let merged = merge_top_scopes(&top_scopes);
    assert_eq!(merged.len(), 2);

    assert_eq!(
        merged[0].record,
        Record {
            start_ns: 100,
            duration_ns: 2 * 100,
            id: "a",
            location: "",
            data: ""
        }
    );
    assert_eq!(
        merged[1].record,
        Record {
            start_ns: 300, // moved forward to make place for "a" (as are all children)
            duration_ns: 2 * 700,
            id: "b",
            location: "",
            data: ""
        }
    );
    assert_eq!(merged[1].pieces.len(), 2);

    let b_merged = merge_children_of_pieces(&stream, &merged[1]).unwrap();
    assert_eq!(b_merged.len(), 2);
    assert_eq!(b_merged[0].record.id, "ba");
    assert_eq!(b_merged[0].record.start_ns, 500);
    assert_eq!(b_merged[0].record.duration_ns, 2 * 200);
    assert_eq!(b_merged[1].record.id, "bb");
    assert_eq!(b_merged[1].record.start_ns, 900);
    assert_eq!(b_merged[1].record.duration_ns, 2 * 200);

    let bb_merged = merge_children_of_pieces(&stream, &b_merged[1]).unwrap();
    assert_eq!(bb_merged.len(), 1);
    assert_eq!(bb_merged[0].record.id, "bba");
    assert_eq!(bb_merged[0].record.start_ns, 900);
    assert_eq!(bb_merged[0].record.duration_ns, 2 * 100);
}
