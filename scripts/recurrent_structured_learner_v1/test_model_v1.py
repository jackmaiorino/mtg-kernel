#!/usr/bin/env python3
"""Focused tests for recurrent structured batching and invariants."""

from __future__ import annotations

import copy
import unittest

import torch

import model_v1


def _row(seed: int, objects: int = 4, actions: int = 3) -> dict:
    generator = torch.Generator().manual_seed(seed)
    return {
        "state": torch.randn(model_v1.STATE_DIM, generator=generator),
        "history_features": torch.randn(
            seed % 4, model_v1.HISTORY_DIM, generator=generator
        ),
        "object_features": torch.randn(
            objects, model_v1.OBJECT_DIM, generator=generator
        ),
        "object_card_ids": torch.arange(objects),
        "object_groups": torch.arange(objects) % model_v1.GROUP_VOCAB,
        "edge_features": torch.randn(2, model_v1.EDGE_DIM, generator=generator),
        "edge_src": torch.tensor([0, 1]),
        "edge_tgt": torch.tensor([1, 2]),
        "action_features": torch.randn(
            actions, model_v1.ACTION_DIM, generator=generator
        ),
        "action_ref_features": torch.randn(
            2, model_v1.REF_DIM, generator=generator
        ),
        "ref_action_indices": torch.tensor([0, min(1, actions - 1)]),
        "ref_node_indices": torch.tensor([0, min(2, objects - 1)]),
        "old_logits": torch.randn(actions, generator=generator),
        "old_value": torch.randn((), generator=generator),
        "selected_index": min(1, actions - 1),
        "substep_count": 1,
    }


class ModelTests(unittest.TestCase):
    def setUp(self) -> None:
        torch.manual_seed(99)
        self.model = model_v1.RecurrentStructuredActorCritic(32).eval()

    def test_ragged_batch_matches_individual_rows(self) -> None:
        rows = [_row(1, 4, 3), _row(2, 6, 5)]
        with torch.no_grad():
            batched_logits, batched_value = self.model(
                model_v1.pack_rows(rows, torch.device("cpu"))
            )
            for index, row in enumerate(rows):
                logits, value = self.model(
                    model_v1.pack_rows([row], torch.device("cpu"))
                )
                count = row["action_features"].shape[0]
                self.assertTrue(
                    torch.allclose(
                        batched_logits[index, :count], logits[0, :count], atol=2e-6
                    )
                )
                self.assertTrue(torch.allclose(batched_value[index], value[0], atol=2e-6))

    def test_object_permutation_is_invariant(self) -> None:
        row = _row(4, 5, 4)
        permutation = torch.tensor([2, 4, 0, 1, 3])
        inverse = torch.empty_like(permutation)
        inverse[permutation] = torch.arange(permutation.shape[0])
        changed = copy.deepcopy(row)
        for key in ("object_features", "object_card_ids", "object_groups"):
            changed[key] = row[key][permutation]
        changed["edge_src"] = inverse[row["edge_src"]]
        changed["edge_tgt"] = inverse[row["edge_tgt"]]
        changed["ref_node_indices"] = inverse[row["ref_node_indices"]]
        with torch.no_grad():
            left = self.model(model_v1.pack_rows([row], torch.device("cpu")))[0]
            right = self.model(model_v1.pack_rows([changed], torch.device("cpu")))[0]
        self.assertTrue(torch.allclose(left, right, atol=2e-6))

    def test_reference_and_digest_channels_activate(self) -> None:
        row = _row(7)
        packed = model_v1.pack_rows([row], torch.device("cpu"))
        with torch.no_grad():
            original_logits, original_value = self.model(packed)
            no_refs, _ = self.model(
                model_v1.pack_rows([row], torch.device("cpu"), remove_refs=True)
            )
            no_digest_logits, no_digest_value = self.model(
                packed, remove_digest=True
            )
        self.assertGreater(float((original_logits - no_refs).abs().max()), 0.0)
        self.assertGreater(
            max(
                float((original_logits - no_digest_logits).abs().max()),
                float((original_value - no_digest_value).abs().max()),
            ),
            0.0,
        )


if __name__ == "__main__":
    unittest.main()
