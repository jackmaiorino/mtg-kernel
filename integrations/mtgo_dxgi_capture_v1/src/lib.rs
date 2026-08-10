use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRectV1 {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl SignedRectV1 {
    pub fn width(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.right
                .checked_sub(self.left)
                .ok_or("rectangle width overflow")?,
        )
        .map_err(|_| "rectangle width is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle width is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn height(self) -> Result<u32, &'static str> {
        u32::try_from(
            self.bottom
                .checked_sub(self.top)
                .ok_or("rectangle height overflow")?,
        )
        .map_err(|_| "rectangle height is not positive")
        .and_then(|value| {
            if value == 0 {
                Err("rectangle height is zero")
            } else {
                Ok(value)
            }
        })
    }

    pub fn contains(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && other.left >= self.left
            && other.top >= self.top
            && other.right <= self.right
            && other.bottom <= self.bottom
    }

    pub fn intersects(self, other: Self) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && other.width().is_ok()
            && other.height().is_ok()
            && self.left < other.right
            && self.right > other.left
            && self.top < other.bottom
            && self.bottom > other.top
    }

    pub fn contains_point(self, x: i32, y: i32) -> bool {
        self.width().is_ok()
            && self.height().is_ok()
            && x >= self.left
            && x < self.right
            && y >= self.top
            && y < self.bottom
    }

    pub fn crop_box_within(self, output: Self) -> Result<CropBoxV1, &'static str> {
        if !output.contains(self) {
            return Err("crop is not wholly contained in the output");
        }
        Ok(CropBoxV1 {
            left: u32::try_from(self.left - output.left).map_err(|_| "crop left is negative")?,
            top: u32::try_from(self.top - output.top).map_err(|_| "crop top is negative")?,
            width: self.width()?,
            height: self.height()?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CropBoxV1 {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

pub fn copy_tightly_packed_bgra8_v1(
    mapped: &[u8],
    row_pitch: usize,
    width: u32,
    height: u32,
) -> Result<Vec<u8>, &'static str> {
    let tight_row = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(4))
        .ok_or("tight row size overflow")?;
    let height = usize::try_from(height).map_err(|_| "height does not fit usize")?;
    if tight_row == 0 || height == 0 || row_pitch < tight_row {
        return Err("mapped texture geometry is invalid");
    }
    let required = row_pitch
        .checked_mul(height)
        .ok_or("mapped byte length overflow")?;
    if mapped.len() < required {
        return Err("mapped texture is shorter than its declared rows");
    }
    let output_len = tight_row
        .checked_mul(height)
        .ok_or("canonical byte length overflow")?;
    let mut output = Vec::with_capacity(output_len);
    for row in 0..height {
        let start = row * row_pitch;
        output.extend_from_slice(&mapped[start..start + tight_row]);
    }
    Ok(output)
}

pub fn sha256_hex_v1(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negative_origin_crop_is_exact() {
        let output = SignedRectV1 {
            left: -1_920,
            top: -200,
            right: 0,
            bottom: 880,
        };
        let client = SignedRectV1 {
            left: -1_800,
            top: -100,
            right: -600,
            bottom: 700,
        };
        assert_eq!(
            client.crop_box_within(output).unwrap(),
            CropBoxV1 {
                left: 120,
                top: 100,
                width: 1_200,
                height: 800,
            }
        );
    }

    #[test]
    fn offscreen_edge_and_invalid_rectangles_fail() {
        let output = SignedRectV1 {
            left: 0,
            top: 0,
            right: 1_920,
            bottom: 1_080,
        };
        assert!(SignedRectV1 {
            left: -1,
            top: 0,
            right: 100,
            bottom: 100,
        }
        .crop_box_within(output)
        .is_err());
        assert!(SignedRectV1 {
            left: 10,
            top: 10,
            right: 10,
            bottom: 20,
        }
        .crop_box_within(output)
        .is_err());
    }

    #[test]
    fn touching_edges_do_not_intersect() {
        let left = SignedRectV1 {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let right = SignedRectV1 {
            left: 100,
            top: 0,
            right: 200,
            bottom: 100,
        };
        assert!(!left.intersects(right));
    }

    #[test]
    fn canonical_copy_ignores_staging_padding() {
        let mapped = [
            1, 2, 3, 4, 5, 6, 7, 8, 90, 91, 92, 93, 9, 10, 11, 12, 13, 14, 15, 16, 94, 95, 96, 97,
        ];
        assert_eq!(
            copy_tightly_packed_bgra8_v1(&mapped, 12, 2, 2).unwrap(),
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
        );
    }
}
