//! Vector reverse mode for the D2 layers. One tape lives for one chair.
use super::*;

type Id = usize;
enum Op {
    Constant,
    Card {
        row: usize,
        embedding: T,
        projection: T,
    },
    Linear {
        input: Id,
        weight: T,
        bias: Option<T>,
        tanh: bool,
    },
    Join(Vec<Id>),
    Mean(Vec<Id>),
}

struct Node {
    y: Vec<f32>,
    dy: Vec<f32>,
    op: Op,
}
pub(super) struct Tape<'a> {
    model: &'a ModelV1,
    rows: &'a ModelCardRowsV1,
    nodes: Vec<Node>,
    // Hidden representations are shared by all paths through a card in a chair.
    cards: [Vec<Option<Id>>; 2],
}

impl<'a> Tape<'a> {
    pub(super) fn new(model: &'a ModelV1, rows: &'a ModelCardRowsV1) -> Self {
        Self {
            model,
            rows,
            nodes: Vec::new(),
            cards: std::array::from_fn(|_| vec![None; model.index_rows]),
        }
    }
    fn node(&mut self, y: Vec<f32>, op: Op) -> Id {
        let id = self.nodes.len();
        self.nodes.push(Node {
            dy: vec![0.; y.len()],
            y,
            op,
        });
        id
    }
    pub(super) fn constant(&mut self, data: Vec<f32>) -> Id {
        self.node(data, Op::Constant)
    }
    pub(super) fn output(&self, id: Id) -> &[f32] {
        &self.nodes[id].y
    }
    pub(super) fn seed(&mut self, id: Id, d: &[f32]) {
        assert_eq!(d.len(), self.nodes[id].dy.len());
        for (slot, value) in self.nodes[id].dy.iter_mut().zip(d) {
            *slot += value;
        }
    }
    pub(super) fn linear(&mut self, input: Id, weight: T, bias: Option<T>, tanh: bool) -> Id {
        let layer = self.model.layer(weight, bias);
        let mut y = vec![0.; layer.out_width];
        layer.forward(&self.nodes[input].y, &mut y);
        if tanh {
            MATH.tanh_in_place_v1(&mut y);
        }
        self.node(
            y,
            Op::Linear {
                input,
                weight,
                bias,
                tanh,
            },
        )
    }
    pub(super) fn join(&mut self, parts: &[Id]) -> Id {
        let data = parts
            .iter()
            .flat_map(|&i| self.output(i).iter().copied())
            .collect();
        self.node(data, Op::Join(parts.to_vec()))
    }
    fn mean(&mut self, parts: Vec<Id>) -> Id {
        let mut data = vec![0.; H];
        for &id in &parts {
            for (a, b) in data.iter_mut().zip(self.output(id)) {
                *a += *b;
            }
        }
        if !parts.is_empty() {
            let scale = 1. / parts.len() as f32;
            for x in &mut data {
                *x *= scale;
            }
        }
        self.node(data, Op::Mean(parts))
    }
    pub(super) fn card(&mut self, row: usize, trunk: TrunkV1) -> Id {
        let (slot, ids) = match trunk {
            TrunkV1::Build => (0, T::BUILD_TRUNK),
            TrunkV1::Pick => (1, T::PICK_TRUNK),
        };
        if let Some(id) = self.cards[slot][row] {
            return id;
        }
        let [embedding, projection, w1, b1, w2, b2] = ids;
        let input = self.node(
            self.model.card_input(self.rows, row, embedding, projection),
            Op::Card {
                row,
                embedding,
                projection,
            },
        );
        let id = self.mlp(input, w1, b1, w2, b2);
        self.cards[slot][row] = Some(id);
        id
    }
    pub(super) fn card_mean(&mut self, sorted_rows: &[usize], trunk: TrunkV1) -> Id {
        let parts = sorted_rows
            .iter()
            .map(|&row| self.card(row, trunk))
            .collect();
        self.mean(parts)
    }
    pub(super) fn mlp(&mut self, x: Id, w1: T, b1: T, w2: T, b2: T) -> Id {
        let first = self.linear(x, w1, Some(b1), true);
        self.linear(first, w2, Some(b2), true)
    }
    pub(super) fn scorer(&mut self, parts: &[Id], w1: T, b1: T, w2: T) -> Id {
        let x = self.join(parts);
        let h = self.linear(x, w1, Some(b1), true);
        self.linear(h, w2, None, false)
    }
    pub(super) fn backward(mut self, grads: &mut [Vec<f32>]) {
        // Visit consumers backwards to obtain all adjoints before a producer.
        // Fold their contributions in forward (decision and candidate) order.
        let mut pending = vec![Vec::<Vec<f32>>::new(); self.nodes.len()];
        for i in (0..self.nodes.len()).rev() {
            let (earlier, later) = self.nodes.split_at_mut(i);
            let node = &mut later[0];
            for contribution in pending[i].drain(..).rev() {
                accumulate(&mut node.dy, &contribution, 1.);
            }
            if node.dy.iter().all(|&d| d == 0.) {
                continue;
            }
            match &node.op {
                Op::Constant | Op::Card { .. } => {}
                Op::Linear {
                    input,
                    weight,
                    tanh,
                    ..
                } => {
                    let width = earlier[*input].y.len();
                    let mut dx = vec![0.; width];
                    for ((d, &y), row) in node
                        .dy
                        .iter_mut()
                        .zip(&node.y)
                        .zip(self.model.t(*weight).chunks_exact(width))
                    {
                        if *tanh {
                            *d *= 1. - y * y;
                        }
                        accumulate(&mut dx, row, *d);
                    }
                    pending[*input].push(dx);
                }
                Op::Mean(inputs) => {
                    let scale = 1. / inputs.len().max(1) as f32;
                    let dx: Vec<_> = node.dy.iter().map(|d| d * scale).collect();
                    for &id in inputs {
                        pending[id].push(dx.clone());
                    }
                }
                Op::Join(inputs) => {
                    let mut offset = 0;
                    for &id in inputs {
                        let end = offset + earlier[id].y.len();
                        pending[id].push(node.dy[offset..end].to_vec());
                        offset = end;
                    }
                }
            }
        }
        // Accumulate shared parameter gradients in forward order too.
        for node in &self.nodes {
            if node.dy.iter().all(|&d| d == 0.) {
                continue;
            }
            match node.op {
                Op::Card {
                    row,
                    embedding,
                    projection,
                } => {
                    for (k, &d) in node.dy[..EMBEDDING_WIDTH_V1].iter().enumerate() {
                        grads[embedding.index()][row * EMBEDDING_WIDTH_V1 + k] += d;
                        let start = k * self.model.table_width;
                        accumulate(
                            &mut grads[projection.index()][start..start + self.model.table_width],
                            self.rows.scaled_row(row),
                            d,
                        );
                    }
                }
                Op::Linear {
                    input,
                    weight,
                    bias,
                    ..
                } => {
                    let x = &self.nodes[input].y;
                    for (&d, g) in node
                        .dy
                        .iter()
                        .zip(grads[weight.index()].chunks_exact_mut(x.len()))
                    {
                        accumulate(g, x, d);
                    }
                    if let Some(b) = bias {
                        accumulate(&mut grads[b.index()], &node.dy, 1.);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Adjoint contribution of one affine output or a scaled identity.
pub(super) fn accumulate(target: &mut [f32], x: &[f32], d: f32) {
    debug_assert_eq!(target.len(), x.len());
    for (slot, &v) in target.iter_mut().zip(x) {
        *slot += d * v;
    }
}
