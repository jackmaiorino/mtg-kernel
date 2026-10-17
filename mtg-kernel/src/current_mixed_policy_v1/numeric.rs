//! Lossless numeric trees. Integer limbs, bytes and tags are exactly representable in f32.
use super::{Error, Result};
use serde_json::Value;

pub const NODE_WIDTH: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericTree {
    // Preorder rows: node kind followed by exact payload limbs.
    rows: Vec<[u16; NODE_WIDTH]>,
    // Large topology ordinals are separate u64 limbs, never truncated to u16.
    topology: Vec<([u16; 4], [u16; 4])>,
}

pub fn limbs(value: u64) -> [u16; 4] {
    std::array::from_fn(|i| (value >> (16 * i)) as u16)
}

impl NumericTree {
    pub fn rows(&self) -> &[[u16; NODE_WIDTH]] {
        &self.rows
    }
    pub fn topology(&self) -> &[([u16; 4], [u16; 4])] {
        &self.topology
    }
    pub fn tensors(&self) -> Vec<[f32; NODE_WIDTH + 8]> {
        self.rows
            .iter()
            .zip(&self.topology)
            .map(|(row, (parent, ordinal))| {
                let mut out = [0.; NODE_WIDTH + 8];
                for (to, from) in out.iter_mut().zip(row.iter().chain(parent).chain(ordinal)) {
                    *to = f32::from(*from);
                }
                out
            })
            .collect()
    }
    pub(super) fn encode(value: &Value) -> Result<Self> {
        let mut tree = Self {
            rows: Vec::new(),
            topology: Vec::new(),
        };
        tree.visit(value, 0, 0)?;
        Ok(tree)
    }
    fn push(&mut self, kind: u16, parent: u64, ordinal: u64, payload: &[u16]) -> Result<u64> {
        let index = u64::try_from(self.rows.len()).map_err(|_| Error::Range)?;
        let mut row = [0; NODE_WIDTH];
        row[0] = kind;
        row[1..1 + payload.len()].copy_from_slice(payload);
        self.rows.push(row);
        self.topology.push((limbs(parent), limbs(ordinal)));
        index.checked_add(1).ok_or(Error::Range)
    }
    fn bytes(&mut self, kind: u16, text: &str, parent: u64, ordinal: u64) -> Result<()> {
        let head = self.push(kind, parent, ordinal, &limbs(text.len() as u64))?;
        for (at, byte) in text.bytes().enumerate() {
            self.push(8, head, at as u64, &[u16::from(byte)])?;
        }
        Ok(())
    }
    fn visit(&mut self, value: &Value, parent: u64, ordinal: u64) -> Result<()> {
        match value {
            Value::Null => {
                self.push(0, parent, ordinal, &[])?;
            }
            Value::Bool(v) => {
                self.push(1, parent, ordinal, &[u16::from(*v)])?;
            }
            Value::Number(v) => {
                if let Some(v) = v.as_u64() {
                    self.push(2, parent, ordinal, &limbs(v))?;
                } else if let Some(v) = v.as_i64() {
                    self.push(3, parent, ordinal, &limbs(v.unsigned_abs()))?;
                } else {
                    return Err(Error::NonIntegralOperand);
                }
            }
            Value::String(v) => self.bytes(4, v, parent, ordinal)?,
            Value::Array(values) => {
                let head = self.push(5, parent, ordinal, &limbs(values.len() as u64))?;
                for (at, value) in values.iter().enumerate() {
                    self.visit(value, head, at as u64)?;
                }
            }
            Value::Object(values) => {
                let head = self.push(6, parent, ordinal, &limbs(values.len() as u64))?;
                // Explicit ordering, independent of serde_json's preserve_order feature.
                let mut fields: Vec<_> = values.iter().collect();
                fields.sort_by(|a, b| a.0.cmp(b.0));
                for (at, (key, value)) in fields.into_iter().enumerate() {
                    let field = self.push(7, head, at as u64, &[])?;
                    self.bytes(9, key, field, 0)?;
                    self.visit(value, field, 1)?;
                }
            }
        }
        Ok(())
    }
}
