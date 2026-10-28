from __future__ import annotations

from collections import Counter
from copy import deepcopy
from dataclasses import asdict, replace
import json
from pathlib import Path
import sys
import unittest


REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "python/tools"))
import limited_decks_v1 as limited
import limited_draft_v1 as draft
import limited_pool_v1 as construction


def supplied(*rounds: tuple[str, ...]) -> tuple[tuple[limited.ImportedDeck, ...], ...]:
    return tuple(tuple(limited.parse_dck(pack) for pack in packs) for packs in rounds)


def visible_without_session(view: draft.DraftView) -> dict:
    value = asdict(view)
    if value["prompt"] is not None:
        del value["prompt"]["session_id"]
    return value


class SuppliedDraftTests(unittest.TestCase):
    def choose(self, session: draft.DraftSession, seat: int, index: int = 0) -> draft.DraftView:
        return session.pick(seat, session.view(seat).prompt, index)

    def finish(self, session: draft.DraftSession) -> None:
        while not session.complete:
            for seat in range(session.seat_count):
                if session.view(seat).prompt is not None:
                    self.choose(session, seat)

    def test_three_seat_alternating_passes_round_transitions_and_order(self) -> None:
        session = draft.DraftSession(supplied(
            ("3 Forest", "3 Island", "3 Mountain"),
            ("2 Plains", "2 Swamp", "2 Llanowar Elves"),
            ("1 Healer's Hawk", "1 Forest", "1 Island"),
        ))
        expected = [[] for _ in range(3)]
        round_names = (("Forest", "Island", "Mountain"), ("Plains", "Swamp", "Llanowar Elves"),
                       ("Healer's Hawk", "Forest", "Island"))
        generation = 0
        for round_index, size in enumerate((3, 2, 1)):
            direction = 1 if round_index % 2 == 0 else -1
            for pick in range(size):
                for seat in range(3):
                    view = session.view(seat)
                    self.assertEqual((view.prompt.round, view.prompt.pick, view.prompt.generation),
                                     (round_index, pick, generation))
                    self.assertEqual(len(view.pack), size - pick)
                    card = view.pack[0]
                    self.assertEqual(card, limited.DeckEntry(1, round_names[round_index][(seat - direction * pick) % 3]))
                    expected[seat].append(card)
                # An arbitrary host arrival order cannot affect pass direction.
                for seat in (2, 0, 1):
                    self.choose(session, seat)
                    self.assertEqual(session.record()["picks"][-1]["copy_id"],
                                     [round_index, (seat - direction * pick) % 3, pick])
                generation += 1
        self.assertTrue(session.complete)
        for seat in range(3):
            view = session.view(seat)
            self.assertEqual(view.picked, tuple(expected[seat]))
            self.assertEqual((view.pack, view.prompt), ((), None))
            self.assertEqual(session.acquired_pool(seat).mainboard, tuple(expected[seat]))

    def test_packs_wait_at_barrier_and_other_seat_prompts_stay_valid(self) -> None:
        session = draft.DraftSession(supplied(("1 Forest\n1 Island", "1 Swamp\n1 Mountain", "2 Plains")))
        pending = [session.view(seat) for seat in range(3)]
        after = self.choose(session, 0, 1)
        self.assertIsNone(after.prompt)
        self.assertEqual(after.pack[0].name, "Forest")
        self.assertEqual(session.view(1), pending[1])
        self.assertEqual(session.view(2), pending[2])
        session.pick(2, pending[2].prompt, 0)
        session.pick(1, pending[1].prompt, 0)
        self.assertEqual(session.view(1).pack[0].name, "Forest")
        self.assertEqual(session.view(1).prompt.pick, 1)

    def test_equal_positive_sizes_required_per_round_but_not_across_rounds(self) -> None:
        invalid = [(), ((limited.parse_dck("1 Forest"),),),
                   supplied(("1 Forest", "2 Island")),
                   ((limited.ImportedDeck(None, (), ()), limited.ImportedDeck(None, (), ())),),
                   supplied(("1 Forest", "1 Island"), ("1 Forest", "1 Island", "1 Swamp"))]
        for packs in invalid:
            with self.subTest(packs=packs), self.assertRaises(ValueError):
                draft.DraftSession(packs)
        session = draft.DraftSession(supplied(("1 Forest", "1 Island"), ("2 Forest", "2 Island")))
        self.finish(session)
        self.assertEqual(len(session.acquired_pool(0).mainboard), 3)

    def test_pack_rows_validate_counts_frozen_membership_and_annotations(self) -> None:
        for count in (0, -1, True, 1.2, "1"):
            with self.subTest(count=count), self.assertRaisesRegex(ValueError, "positive integer"):
                bad = limited.ImportedDeck(None, (limited.DeckEntry(count, "Forest"),), ())
                draft.DraftSession(((bad, bad),))
        for name in ("Never Implemented", "Zombie Token", "Annul"):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, "outside the frozen FDN"):
                draft.DraftSession(supplied((f"1 {name}", "1 Forest")))
        bad = limited.ImportedDeck(None, (limited.DeckEntry(1, "Forest", ""),), ())
        with self.assertRaisesRegex(ValueError, "printing annotation"):
            draft.DraftSession(((bad, bad),))
        with self.assertRaisesRegex(ValueError, "row sequences"):
            draft.DraftSession(((limited.ImportedDeck(None, None, ()), bad),))

    def test_duplicate_physical_copies_conserved_through_wraparound_and_every_pick(self) -> None:
        packs = supplied(("2 [FDN:227] Llanowar Elves\nSB: 3 Llanowar Elves",) * 3,
                         ("2 Forest",) * 3)
        session = draft.DraftSession(packs)
        expected = {(round_index, seat, copy) for round_index, size in enumerate((5, 2))
                    for seat in range(3) for copy in range(size)}
        while not session.complete:
            for seat in (1, 2, 0):
                self.choose(session, seat)
                # Physical custody is checked only in trusted host state/records.
                recorded = session.record()["picks"]
                copies = [tuple(pick["copy_id"]) for pick in recorded]
                copies += [card.copy_id for pack in session._packs for card in pack]
                copies += [copy for copy in expected if copy[0] > session._round]
                self.assertEqual(Counter(copies), Counter({copy: 1 for copy in expected}))
                for drafter in range(3):
                    self.assertEqual([card.copy_id for card in session._picked[drafter]],
                                     [tuple(pick["copy_id"]) for pick in recorded if pick["seat"] == drafter])
                if session.complete:
                    break
        cards = [card for seat in range(3) for card in session.view(seat).picked]
        self.assertEqual(sum(card.name == "Llanowar Elves" for card in cards), 15)
        self.assertEqual(sum(card.printing == "FDN:227" for card in cards), 6)

    def test_stale_foreign_duplicate_and_bad_index_refusals_do_not_mutate(self) -> None:
        packs = supplied(("2 Forest", "2 Island", "2 Swamp"), ("1 Mountain",) * 3)
        session, foreign = draft.DraftSession(packs), draft.DraftSession(packs)
        view = session.view(0)
        bad_inputs = [(0, foreign.view(0).prompt, 0), (1, view.prompt, 0),
                      (0, replace(view.prompt, round=1), 0), (0, replace(view.prompt, generation=True), 0),
                      (0, view.prompt, -1), (0, view.prompt, 2), (0, view.prompt, True),
                      (0, view.prompt, 0.0), (True, view.prompt, 0), (3, view.prompt, 0)]
        for seat, prompt, index in bad_inputs:
            before, views = session.record(), [session.view(i) for i in range(3)]
            with self.subTest(seat=seat, prompt=prompt, index=index), self.assertRaises(ValueError):
                session.pick(seat, prompt, index)
            self.assertEqual(session.record(), before)
            self.assertEqual([session.view(i) for i in range(3)], views)
        self.choose(session, 0)
        for advance in (False, True):
            if advance:
                self.choose(session, 1)
                self.choose(session, 2)
            before = session.record()
            with self.assertRaisesRegex(ValueError, "already answered"):
                session.pick(0, view.prompt, 0)
            self.assertEqual(session.record(), before)

    def test_actor_view_independent_of_unseen_packs_picks_and_provenance(self) -> None:
        left = draft.DraftSession(supplied(("2 Forest", "2 Island", "2 Swamp"), ("1 Plains",) * 3),
                                  seed=1, provenance="private-left-input")
        right = draft.DraftSession(supplied(("2 Forest", "2 Mountain", "2 Llanowar Elves"), ("1 Healer's Hawk",) * 3),
                                   seed=99, provenance="private-right-input")
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))
        self.choose(left, 1, 0)
        self.choose(right, 1, 1)
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))
        self.assertEqual(set(asdict(left.view(0))), {"seat", "complete", "prompt", "pack", "picked"})
        self.assertNotIn("private-left-input", repr(left.view(0)))
        self.assertNotIn("Island", repr(left.view(0)))
        self.choose(left, 0)
        self.choose(right, 0)
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))

    def test_post_pass_view_does_not_disclose_unseen_original_pack_order(self) -> None:
        left = draft.DraftSession(supplied(("1 Forest\n1 Island", "1 Swamp\n1 Mountain")))
        right = draft.DraftSession(supplied(("1 Forest\n1 Island", "1 Mountain\n1 Swamp")))
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))
        # The other drafter takes Mountain from different unseen positions.
        self.choose(left, 1, 1)
        self.choose(right, 1, 0)
        self.choose(left, 0, 0)
        self.choose(right, 0, 0)
        for session in (left, right):
            view = session.view(0)
            self.assertEqual(view.pack, (limited.DeckEntry(1, "Swamp"),))
            self.assertEqual(view.picked, (limited.DeckEntry(1, "Forest"),))
            for card in (*asdict(view)["pack"], *asdict(view)["picked"]):
                self.assertEqual(set(card), {"count", "name", "printing"})
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))
        self.choose(left, 0)
        self.choose(right, 0)
        # The host still binds the different original Swamp physical copies.
        self.assertEqual(left.record()["picks"][-1]["copy_id"], [0, 1, 0])
        self.assertEqual(right.record()["picks"][-1]["copy_id"], [0, 1, 1])
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))
        self.choose(left, 1)
        self.choose(right, 1)
        self.assertTrue(left.complete and right.complete)
        self.assertEqual(visible_without_session(left.view(0)), visible_without_session(right.view(0)))

    def test_json_record_replays_partial_waiting_step_with_fresh_prompts_then_completes(self) -> None:
        original = draft.DraftSession(supplied(("2 Forest", "2 Island", "2 Mountain"), ("1 Plains",) * 3),
                                      seed=-123, provenance="sha256:supplied-file-manifest")
        self.choose(original, 2, 1)
        recorded = json.loads(json.dumps(original.record()))
        restored = draft.DraftSession.replay(recorded)
        self.assertEqual(restored.record(), recorded)
        self.assertEqual((recorded["seed"], recorded["provenance"]), (-123, "sha256:supplied-file-manifest"))
        for seat in range(3):
            self.assertEqual(visible_without_session(original.view(seat)), visible_without_session(restored.view(seat)))
        with self.assertRaisesRegex(ValueError, "foreign"):
            restored.pick(0, original.view(0).prompt, 0)
        self.finish(original)
        self.finish(restored)
        self.assertEqual(original.record(), restored.record())
        complete = draft.DraftSession.replay(original.record())
        self.assertTrue(complete.complete)
        for seat in range(3):
            self.assertEqual(original.acquired_pool(seat), complete.acquired_pool(seat))

    def test_replay_refuses_input_index_copy_step_and_closed_shape_tampering(self) -> None:
        session = draft.DraftSession(supplied(("2 Forest", "2 Island")))
        self.choose(session, 0)
        record = session.record()
        mutations = [lambda r: r["packs"][0][1]["mainboard"][0].update(name="Swamp"),
                     lambda r: r["picks"][0].update(index=1),
                     lambda r: r["picks"][0].update(copy_id=[0, 1, 0]),
                     lambda r: r["picks"][0].update(pick=1),
                     lambda r: r["picks"][0].update(seat=True),
                     lambda r: r["picks"].append(deepcopy(r["picks"][0])),
                     lambda r: r.update(whole_world={}),
                     lambda r: r["packs"][0][0]["mainboard"][0].update(card_id=1)]
        for mutate in mutations:
            changed = deepcopy(record)
            mutate(changed)
            before = deepcopy(changed)
            with self.subTest(record=changed), self.assertRaises(ValueError):
                draft.DraftSession.replay(changed)
            self.assertEqual(changed, before)
        self.assertEqual(session.record(), record)

    def test_input_containers_and_host_record_do_not_alias_live_state(self) -> None:
        rows = [limited.DeckEntry(2, "Forest")]
        pack = limited.ImportedDeck("Host-only pack name", rows, [])
        session = draft.DraftSession([[pack, pack]])
        before = session.record()
        rows.clear()
        exported = session.record()
        exported["packs"][0][0]["mainboard"].clear()
        self.assertEqual(session.record(), before)
        self.assertNotIn("Host-only pack name", repr(session.view(0)))
        with self.assertRaisesRegex(ValueError, "complete"):
            session.acquired_pool(0)
        self.assertEqual(session.record(), before)

    def test_completed_pools_feed_construction_with_explicit_basics_for_each_seat(self) -> None:
        session = draft.DraftSession(supplied(("5 [FDN:227] Llanowar Elves",) * 3))
        self.finish(session)
        for seat in range(3):
            acquired = session.acquired_pool(seat)
            partition = limited.parse_dck("5 Llanowar Elves\n35 Forest")
            with self.assertRaisesRegex(ValueError, "not owned or explicitly added"):
                construction.construct_deck(acquired, partition)
            built = construction.construct_deck(acquired, partition, (limited.DeckEntry(35, "Forest"),))
            self.assertEqual(len(built.mainboard_card_ids), 40)
            self.assertEqual(len(acquired.mainboard), 5)
            self.assertTrue(all(row.count == 1 and row.printing == "FDN:227" for row in acquired.mainboard))

    def test_drafting_partial_metadata_cannot_grant_executable_authority(self) -> None:
        session = draft.DraftSession(supplied(("1 Muldrotha, the Gravetide",) * 2))
        self.finish(session)
        for seat in range(2):
            acquired = session.acquired_pool(seat)
            with self.assertRaisesRegex(ValueError, r"Muldrotha, the Gravetide \(partial\)"):
                construction.construct_deck(acquired, limited.parse_dck("1 Muldrotha, the Gravetide\n39 Forest"),
                                            (limited.DeckEntry(39, "Forest"),))
            built = construction.construct_deck(acquired, limited.parse_dck("40 Forest\nSB: 1 Muldrotha, the Gravetide"),
                                                 (limited.DeckEntry(40, "Forest"),))
            self.assertEqual(len(built.mainboard_card_ids), 40)


if __name__ == "__main__":
    unittest.main()
