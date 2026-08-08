//! Pure native training Store V2 layout and stage-leaf grammar authority.
//!
//! This module owns the exact authoritative final basenames, their directory
//! placement, the eight-digit index grammar, and the literal `.B.stage-v2`
//! same-parent stage grammar. It performs no filesystem access and makes no
//! durability, publication, locking, or recovery claim. Stage recognition
//! first validates the final-basename grammar and then applies the literal
//! leading dot and `.stage-v2` suffix; nothing else is a recognized stage.

use std::error::Error;
use std::fmt::{Display, Formatter};

/// Exact persistent lock leaf published at the Store root.
pub const NATIVE_TRAINING_STORE_LOCK_LEAF_V2: &str = ".mtg-kernel-native-train.lock";
/// Literal stage suffix applied after a validated final basename.
pub const NATIVE_TRAINING_STORE_STAGE_SUFFIX_V2: &str = ".stage-v2";
/// Largest representable durable index in the eight-digit grammar.
pub const NATIVE_TRAINING_STORE_MAX_UPDATE_INDEX_V2: u64 = 99_999_999;

const RUN_LEAF_V2: &str = "run.json";
const LATEST_LEAF_V2: &str = "latest.json";
const SEGMENT_PREFIX_V2: &str = "segment-";
const CONTINUATION_INFIX_V2: &str = ".continuation-";
const UPDATE_PREFIX_V2: &str = "update-";
const JSON_SUFFIX_V2: &str = ".json";
const CHECKPOINT_SUFFIX_V2: &str = ".checkpoint.json";
const STATE_PAYLOAD_SUFFIX_V2: &str = ".state.f32le";
const SIDECAR_SUFFIX_V2: &str = ".sidecar.json";
const HEAD_SUFFIX_V2: &str = ".head.json";
const REF_SUFFIX_V2: &str = ".ref.json";
const FIXED_INDEX_DIGITS_V2: usize = 8;

/// Authoritative Store directories addressed by the layout grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreDirectoryV2 {
    Root,
    Segments,
    Checkpoints,
    Heads,
    Refs,
}

impl NativeTrainingStoreDirectoryV2 {
    /// Exact child directory basename under the Store root; `None` for the root.
    pub const fn basename(self) -> Option<&'static str> {
        match self {
            Self::Root => None,
            Self::Segments => Some("segments"),
            Self::Checkpoints => Some("checkpoints"),
            Self::Heads => Some("heads"),
            Self::Refs => Some("refs"),
        }
    }
}

/// Fixed directory-creation order for the authoritative subdirectories.
pub const NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2: [NativeTrainingStoreDirectoryV2; 4] = [
    NativeTrainingStoreDirectoryV2::Segments,
    NativeTrainingStoreDirectoryV2::Checkpoints,
    NativeTrainingStoreDirectoryV2::Heads,
    NativeTrainingStoreDirectoryV2::Refs,
];

/// A validated authoritative final name in the Store layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreFinalNameV2 {
    Run,
    Latest,
    SegmentManifest {
        generation_index: u64,
    },
    SegmentContinuation {
        generation_index: u64,
        continuation_index: u64,
    },
    CheckpointManifest {
        generation_index: u64,
    },
    StatePayload {
        generation_index: u64,
    },
    CheckpointSidecar {
        generation_index: u64,
    },
    HeadRecord {
        generation_index: u64,
    },
    CheckpointReference {
        generation_index: u64,
    },
}

impl NativeTrainingStoreFinalNameV2 {
    /// The directory this final name lives in.
    pub const fn directory(self) -> NativeTrainingStoreDirectoryV2 {
        match self {
            Self::Run | Self::Latest => NativeTrainingStoreDirectoryV2::Root,
            Self::SegmentManifest { .. } | Self::SegmentContinuation { .. } => {
                NativeTrainingStoreDirectoryV2::Segments
            }
            Self::CheckpointManifest { .. }
            | Self::StatePayload { .. }
            | Self::CheckpointSidecar { .. } => NativeTrainingStoreDirectoryV2::Checkpoints,
            Self::HeadRecord { .. } => NativeTrainingStoreDirectoryV2::Heads,
            Self::CheckpointReference { .. } => NativeTrainingStoreDirectoryV2::Refs,
        }
    }

    /// Exact final basename, rejecting any index above the eight-digit bound.
    pub fn final_basename(self) -> Result<String> {
        Ok(match self {
            Self::Run => RUN_LEAF_V2.to_owned(),
            Self::Latest => LATEST_LEAF_V2.to_owned(),
            Self::SegmentManifest { generation_index } => format!(
                "{SEGMENT_PREFIX_V2}{}{JSON_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
            Self::SegmentContinuation {
                generation_index,
                continuation_index,
            } => format!(
                "{SEGMENT_PREFIX_V2}{}{CONTINUATION_INFIX_V2}{}{JSON_SUFFIX_V2}",
                fixed_index_v2(generation_index)?,
                fixed_index_v2(continuation_index)?
            ),
            Self::CheckpointManifest { generation_index } => format!(
                "{UPDATE_PREFIX_V2}{}{CHECKPOINT_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
            Self::StatePayload { generation_index } => format!(
                "{UPDATE_PREFIX_V2}{}{STATE_PAYLOAD_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
            Self::CheckpointSidecar { generation_index } => format!(
                "{UPDATE_PREFIX_V2}{}{SIDECAR_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
            Self::HeadRecord { generation_index } => format!(
                "{UPDATE_PREFIX_V2}{}{HEAD_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
            Self::CheckpointReference { generation_index } => format!(
                "{UPDATE_PREFIX_V2}{}{REF_SUFFIX_V2}",
                fixed_index_v2(generation_index)?
            ),
        })
    }

    /// Exact stage basename: a literal leading dot, the validated final
    /// basename, then the literal `.stage-v2` suffix.
    pub fn stage_basename(self) -> Result<String> {
        Ok(format!(
            ".{}{NATIVE_TRAINING_STORE_STAGE_SUFFIX_V2}",
            self.final_basename()?
        ))
    }
}

/// One classified directory leaf under the Store layout grammar.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreLeafV2 {
    /// The persistent lock leaf at the Store root.
    Lock,
    /// A recognized authoritative final name.
    Final(NativeTrainingStoreFinalNameV2),
    /// A recognized non-authoritative stage leaf for the named final.
    Stage(NativeTrainingStoreFinalNameV2),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeTrainingStoreLayoutV2ErrorKind {
    IndexOutOfRange,
    UnknownLeaf,
    MalformedStageLeaf,
}

impl NativeTrainingStoreLayoutV2ErrorKind {
    pub const fn code(self) -> &'static str {
        match self {
            Self::IndexOutOfRange => "native-training-store-layout-index-out-of-range",
            Self::UnknownLeaf => "native-training-store-layout-unknown-leaf",
            Self::MalformedStageLeaf => "native-training-store-layout-malformed-stage-leaf",
        }
    }
}

/// Redacted layout error carrying only its classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeTrainingStoreLayoutV2Error {
    kind: NativeTrainingStoreLayoutV2ErrorKind,
}

impl NativeTrainingStoreLayoutV2Error {
    pub const fn kind(self) -> NativeTrainingStoreLayoutV2ErrorKind {
        self.kind
    }

    pub const fn code(self) -> &'static str {
        self.kind.code()
    }
}

impl Display for NativeTrainingStoreLayoutV2Error {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.code())
    }
}

impl Error for NativeTrainingStoreLayoutV2Error {}

type Result<T> = std::result::Result<T, NativeTrainingStoreLayoutV2Error>;

const fn layout_error_v2(
    kind: NativeTrainingStoreLayoutV2ErrorKind,
) -> NativeTrainingStoreLayoutV2Error {
    NativeTrainingStoreLayoutV2Error { kind }
}

fn fixed_index_v2(index: u64) -> Result<String> {
    if index > NATIVE_TRAINING_STORE_MAX_UPDATE_INDEX_V2 {
        return Err(layout_error_v2(
            NativeTrainingStoreLayoutV2ErrorKind::IndexOutOfRange,
        ));
    }
    Ok(format!("{index:0width$}", width = FIXED_INDEX_DIGITS_V2))
}

fn parse_fixed_index_v2(text: &str) -> Option<u64> {
    if text.len() != FIXED_INDEX_DIGITS_V2 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse::<u64>().ok()
}

fn parse_final_leaf_v2(
    directory: NativeTrainingStoreDirectoryV2,
    leaf: &str,
) -> Option<NativeTrainingStoreFinalNameV2> {
    match directory {
        NativeTrainingStoreDirectoryV2::Root => match leaf {
            RUN_LEAF_V2 => Some(NativeTrainingStoreFinalNameV2::Run),
            LATEST_LEAF_V2 => Some(NativeTrainingStoreFinalNameV2::Latest),
            _ => None,
        },
        NativeTrainingStoreDirectoryV2::Segments => {
            let stem = leaf
                .strip_prefix(SEGMENT_PREFIX_V2)?
                .strip_suffix(JSON_SUFFIX_V2)?;
            match stem.split_once(CONTINUATION_INFIX_V2) {
                None => Some(NativeTrainingStoreFinalNameV2::SegmentManifest {
                    generation_index: parse_fixed_index_v2(stem)?,
                }),
                Some((generation, continuation)) => {
                    Some(NativeTrainingStoreFinalNameV2::SegmentContinuation {
                        generation_index: parse_fixed_index_v2(generation)?,
                        continuation_index: parse_fixed_index_v2(continuation)?,
                    })
                }
            }
        }
        NativeTrainingStoreDirectoryV2::Checkpoints => {
            let stem = leaf.strip_prefix(UPDATE_PREFIX_V2)?;
            if let Some(index) = stem.strip_suffix(CHECKPOINT_SUFFIX_V2) {
                return Some(NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: parse_fixed_index_v2(index)?,
                });
            }
            if let Some(index) = stem.strip_suffix(STATE_PAYLOAD_SUFFIX_V2) {
                return Some(NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: parse_fixed_index_v2(index)?,
                });
            }
            let index = stem.strip_suffix(SIDECAR_SUFFIX_V2)?;
            Some(NativeTrainingStoreFinalNameV2::CheckpointSidecar {
                generation_index: parse_fixed_index_v2(index)?,
            })
        }
        NativeTrainingStoreDirectoryV2::Heads => {
            let index = leaf
                .strip_prefix(UPDATE_PREFIX_V2)?
                .strip_suffix(HEAD_SUFFIX_V2)?;
            Some(NativeTrainingStoreFinalNameV2::HeadRecord {
                generation_index: parse_fixed_index_v2(index)?,
            })
        }
        NativeTrainingStoreDirectoryV2::Refs => {
            let index = leaf
                .strip_prefix(UPDATE_PREFIX_V2)?
                .strip_suffix(REF_SUFFIX_V2)?;
            Some(NativeTrainingStoreFinalNameV2::CheckpointReference {
                generation_index: parse_fixed_index_v2(index)?,
            })
        }
    }
}

/// Classify one directory leaf under the layout grammar, failing closed.
///
/// Recognition order: the exact root lock leaf, then a recognized final
/// basename, then a recognized stage leaf whose embedded final basename
/// validates first. A leaf that is stage-shaped (leading dot or trailing
/// `.stage-v2`) without an exactly valid embedded final basename is a
/// malformed stage leaf; everything else is an unknown leaf.
pub fn classify_store_leaf_v2(
    directory: NativeTrainingStoreDirectoryV2,
    leaf: &str,
) -> Result<NativeTrainingStoreLeafV2> {
    if matches!(directory, NativeTrainingStoreDirectoryV2::Root)
        && leaf == NATIVE_TRAINING_STORE_LOCK_LEAF_V2
    {
        return Ok(NativeTrainingStoreLeafV2::Lock);
    }
    if let Some(final_name) = parse_final_leaf_v2(directory, leaf) {
        return Ok(NativeTrainingStoreLeafV2::Final(final_name));
    }
    let stage_shaped =
        leaf.starts_with('.') || leaf.ends_with(NATIVE_TRAINING_STORE_STAGE_SUFFIX_V2);
    let Some(embedded) = leaf
        .strip_prefix('.')
        .and_then(|rest| rest.strip_suffix(NATIVE_TRAINING_STORE_STAGE_SUFFIX_V2))
    else {
        return Err(layout_error_v2(if stage_shaped {
            NativeTrainingStoreLayoutV2ErrorKind::MalformedStageLeaf
        } else {
            NativeTrainingStoreLayoutV2ErrorKind::UnknownLeaf
        }));
    };
    match parse_final_leaf_v2(directory, embedded) {
        Some(final_name) => Ok(NativeTrainingStoreLeafV2::Stage(final_name)),
        None => Err(layout_error_v2(
            NativeTrainingStoreLayoutV2ErrorKind::MalformedStageLeaf,
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_final_basename_and_directory_matches_the_frozen_layout() {
        let cases: [(
            NativeTrainingStoreFinalNameV2,
            NativeTrainingStoreDirectoryV2,
            &str,
        ); 9] = [
            (
                NativeTrainingStoreFinalNameV2::Run,
                NativeTrainingStoreDirectoryV2::Root,
                "run.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::Latest,
                NativeTrainingStoreDirectoryV2::Root,
                "latest.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::SegmentManifest {
                    generation_index: 0,
                },
                NativeTrainingStoreDirectoryV2::Segments,
                "segment-00000000.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::SegmentContinuation {
                    generation_index: 512,
                    continuation_index: 0,
                },
                NativeTrainingStoreDirectoryV2::Segments,
                "segment-00000512.continuation-00000000.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::CheckpointManifest {
                    generation_index: 512,
                },
                NativeTrainingStoreDirectoryV2::Checkpoints,
                "update-00000512.checkpoint.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::StatePayload {
                    generation_index: 512,
                },
                NativeTrainingStoreDirectoryV2::Checkpoints,
                "update-00000512.state.f32le",
            ),
            (
                NativeTrainingStoreFinalNameV2::CheckpointSidecar {
                    generation_index: 512,
                },
                NativeTrainingStoreDirectoryV2::Checkpoints,
                "update-00000512.sidecar.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::HeadRecord {
                    generation_index: 99_999_999,
                },
                NativeTrainingStoreDirectoryV2::Heads,
                "update-99999999.head.json",
            ),
            (
                NativeTrainingStoreFinalNameV2::CheckpointReference {
                    generation_index: 4,
                },
                NativeTrainingStoreDirectoryV2::Refs,
                "update-00000004.ref.json",
            ),
        ];
        for (final_name, directory, expected) in cases {
            assert_eq!(final_name.directory(), directory);
            assert_eq!(final_name.final_basename().unwrap(), expected);
            assert_eq!(
                final_name.stage_basename().unwrap(),
                format!(".{expected}.stage-v2")
            );
            assert_eq!(
                classify_store_leaf_v2(directory, expected).unwrap(),
                NativeTrainingStoreLeafV2::Final(final_name)
            );
            assert_eq!(
                classify_store_leaf_v2(directory, &format!(".{expected}.stage-v2")).unwrap(),
                NativeTrainingStoreLeafV2::Stage(final_name)
            );
        }
    }

    #[test]
    fn indices_above_the_eight_digit_bound_reject_before_formatting() {
        for final_name in [
            NativeTrainingStoreFinalNameV2::SegmentManifest {
                generation_index: NATIVE_TRAINING_STORE_MAX_UPDATE_INDEX_V2 + 1,
            },
            NativeTrainingStoreFinalNameV2::SegmentContinuation {
                generation_index: 0,
                continuation_index: u64::MAX,
            },
            NativeTrainingStoreFinalNameV2::StatePayload {
                generation_index: 100_000_000,
            },
        ] {
            for formatted in [final_name.final_basename(), final_name.stage_basename()] {
                assert_eq!(
                    formatted.unwrap_err().kind(),
                    NativeTrainingStoreLayoutV2ErrorKind::IndexOutOfRange
                );
            }
        }
    }

    #[test]
    fn the_lock_leaf_is_recognized_only_at_the_root() {
        assert_eq!(
            classify_store_leaf_v2(
                NativeTrainingStoreDirectoryV2::Root,
                NATIVE_TRAINING_STORE_LOCK_LEAF_V2
            )
            .unwrap(),
            NativeTrainingStoreLeafV2::Lock
        );
        for directory in NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2 {
            assert_eq!(
                classify_store_leaf_v2(directory, NATIVE_TRAINING_STORE_LOCK_LEAF_V2)
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreLayoutV2ErrorKind::MalformedStageLeaf
            );
        }
    }

    #[test]
    fn misplaced_final_names_fail_closed_per_directory() {
        let misplacements = [
            (NativeTrainingStoreDirectoryV2::Segments, "run.json"),
            (NativeTrainingStoreDirectoryV2::Segments, "latest.json"),
            (
                NativeTrainingStoreDirectoryV2::Root,
                "segment-00000000.json",
            ),
            (
                NativeTrainingStoreDirectoryV2::Heads,
                "update-00000000.checkpoint.json",
            ),
            (
                NativeTrainingStoreDirectoryV2::Refs,
                "update-00000000.head.json",
            ),
            (
                NativeTrainingStoreDirectoryV2::Checkpoints,
                "segment-00000000.json",
            ),
        ];
        for (directory, leaf) in misplacements {
            assert_eq!(
                classify_store_leaf_v2(directory, leaf).unwrap_err().kind(),
                NativeTrainingStoreLayoutV2ErrorKind::UnknownLeaf
            );
        }
    }

    #[test]
    fn index_grammar_rejects_every_non_canonical_spelling() {
        let rejects = [
            "segment-0.json",
            "segment-000000000.json",
            "segment-0000000a.json",
            "segment- 0000000.json",
            "segment-+0000000.json",
            "segment--0000000.json",
            "segment-00000000.JSON",
            "Segment-00000000.json",
            "segment-00000000.json ",
            "segment-00000000.continuation-0.json",
            "segment-00000000.continuation-000000000.json",
        ];
        for leaf in rejects {
            assert_eq!(
                classify_store_leaf_v2(NativeTrainingStoreDirectoryV2::Segments, leaf)
                    .unwrap_err()
                    .kind(),
                NativeTrainingStoreLayoutV2ErrorKind::UnknownLeaf,
                "leaf {leaf:?} must be unknown"
            );
        }
    }

    #[test]
    fn stage_recognition_requires_an_exactly_valid_embedded_final_basename() {
        let malformed = [
            ".segment-0.json.stage-v2",
            ".segment-00000000.json.stage-v3",
            ".segment-00000000.json.stage-v2 ",
            "..segment-00000000.json.stage-v2",
            ".segment-00000000.json.stage-v2.stage-v2",
            "segment-00000000.json.stage-v2",
            ".stage-v2",
            ".hidden",
        ];
        for leaf in malformed {
            let kind = classify_store_leaf_v2(NativeTrainingStoreDirectoryV2::Segments, leaf)
                .unwrap_err()
                .kind();
            assert_eq!(
                kind,
                NativeTrainingStoreLayoutV2ErrorKind::MalformedStageLeaf,
                "leaf {leaf:?} must be a malformed stage leaf"
            );
        }
        assert_eq!(
            classify_store_leaf_v2(NativeTrainingStoreDirectoryV2::Root, ".run.json.stage-v2")
                .unwrap(),
            NativeTrainingStoreLeafV2::Stage(NativeTrainingStoreFinalNameV2::Run)
        );
        assert_eq!(
            classify_store_leaf_v2(
                NativeTrainingStoreDirectoryV2::Root,
                ".latest.json.stage-v2"
            )
            .unwrap(),
            NativeTrainingStoreLeafV2::Stage(NativeTrainingStoreFinalNameV2::Latest)
        );
    }

    #[test]
    fn subdirectory_order_is_frozen() {
        let names: Vec<&str> = NATIVE_TRAINING_STORE_SUBDIRECTORY_ORDER_V2
            .iter()
            .map(|directory| directory.basename().unwrap())
            .collect();
        assert_eq!(names, ["segments", "checkpoints", "heads", "refs"]);
        assert!(NativeTrainingStoreDirectoryV2::Root.basename().is_none());
    }
}
