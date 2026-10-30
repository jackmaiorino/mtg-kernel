"""FlatV7: London pregame decisions on the unchanged FlatV4 tensor contract.

Pauper London mulligans v1 (docs/design/pauper_london_mulligans_v1.md,
section 5). The frozen features_v7.py (FlatV4) encoder supplies every
in-game decision unchanged, so in-game V7 tensors equal V4 tensors. Pregame
decisions (the observation carries ``extensions.london_pregame``) reuse three
FlatV4 columns that no admitted gameplay row uses at a pregame decision:

- action-kind column 15 (V4 ``choose_effect_number``) is ``london_opening``;
  a numeric choice is refused instead of aliased;
- engine-stage slot 9 (V4 ``halted``) is ``london_pregame``;
- at pregame decisions only, the self ``has_lost`` column carries "opponent
  announced a mulligan this round" and the opponent column "opponent has
  kept". In-game rows encode ``has_lost`` exactly as V4.

The canonical observation gains the ``london_pregame`` extension only while
London is pending, so it changes the state hash of pregame decisions only.
No dimension changes: V4 checkpoints transfer by the explicit same-shape
transfer. JSON validation checks consistency; the engine authenticates play.
"""
from __future__ import annotations

import hashlib
import json
from dataclasses import replace
from typing import Any

import torch

from . import features_v7 as predecessor

FeatureSchemaError = predecessor.FeatureSchemaError
FeatureSchema = predecessor.FeatureSchema
EncodedDecision = predecessor.EncodedDecision
FEATURE_SCHEMA_VERSION = "actor-relative-v10-python-1"
FEATURE_REGISTRY_VERSION = "rust-observation-v10-action-v6-registry-1"
ENCODING_CONTRACT_VERSION = "actor-relative-node-graph-19"
MODEL_CONTRACT_VERSION = predecessor.MODEL_CONTRACT_VERSION
STATE_FEATURE_DIM = predecessor.STATE_FEATURE_DIM
OBJECT_FEATURE_DIM = predecessor.OBJECT_FEATURE_DIM
EDGE_FEATURE_DIM = predecessor.EDGE_FEATURE_DIM
ACTION_FEATURE_DIM = predecessor.ACTION_FEATURE_DIM
ACTION_REF_FEATURE_DIM = predecessor.ACTION_REF_FEATURE_DIM
STATE_HASH_DIM = predecessor.STATE_HASH_DIM
ACTION_HASH_DIM = predecessor.ACTION_HASH_DIM

LONDON_OPENING_KIND_COLUMN = 15
ACTION_KINDS = list(predecessor.ACTION_KINDS)
if ACTION_KINDS[LONDON_OPENING_KIND_COLUMN] != "choose_effect_number":
    raise RuntimeError("V4 action-kind column 15 moved")
ACTION_KINDS[LONDON_OPENING_KIND_COLUMN] = "london_opening"
ENGINE_STAGES = list(predecessor.ENGINE_STAGES)
if ENGINE_STAGES[9] != "halted":
    raise RuntimeError("V4 engine-stage slot 9 moved")
ENGINE_STAGES[9] = "london_pregame"
REFUSED_ACTION_KINDS = ("choose_effect_number",)
LONDON_ACTION_KINDS = ("choose_london_mulligan", "choose_london_bottom")
LONDON_PHASES = ("announce", "bottom")
HAND_SIZE = 7

# State columns, from the FlatV4 state layout (features_v7._state_features):
# twelve phases and nine seat features precede two nineteen-column player
# blocks, each with has_lost at offset nine; the engine-stage one-hot starts
# after the combat, known-card and first eight engine-context columns.
_PLAYER_BLOCK = 19
_PLAYER_BLOCK_START = len(predecessor.PHASES) + 9
SELF_HAS_LOST_COLUMN = _PLAYER_BLOCK_START + 9
OPPONENT_HAS_LOST_COLUMN = _PLAYER_BLOCK_START + _PLAYER_BLOCK + 9
ENGINE_STAGE_START = _PLAYER_BLOCK_START + 2 * _PLAYER_BLOCK + 9 + 4 + 2 + 3 + 3
LONDON_PREGAME_STAGE_COLUMN = ENGINE_STAGE_START + 9

# Action columns, from the FlatV4 action layout (features_v7._action_features):
# 27 kinds, 3 actor-seat, 13 card-reference, 2 target-kind and 3 target-seat
# columns precede the 22 numeric columns. London rows keep each reused
# column's V4 scale (the two opponent facts are 0 or 1 over 16).
_NUMERIC_START = len(predecessor.ACTION_KINDS) + 3 + predecessor.CARD_REF_FEATURE_DIM + 2 + 3
REMAINING_COLUMN = _NUMERIC_START + 1          # V4 remaining: cards still to bottom
OPPONENT_KEPT_COLUMN = _NUMERIC_START + 4      # V4 option_index
OPPONENT_ANNOUNCED_COLUMN = _NUMERIC_START + 5  # V4 option_count
OWN_MULLIGANS_COLUMN = _NUMERIC_START + 15     # V4 selected_count
OPPONENT_MULLIGANS_COLUMN = _NUMERIC_START + 18  # V4 number
MULLIGAN_VALUE_COLUMN = _NUMERIC_START + 21    # V4 value

_U8 = 255
LONDON_PREGAME_SPEC = predecessor.ObjectSpec(
    {
        "phase": predecessor.E(list(LONDON_PHASES)),
        "mulligans_taken": predecessor.ListSpec(predecessor.I(predecessor.MODEL_INPUT, maximum=HAND_SIZE), length=2),
        "kept": predecessor.ListSpec(predecessor.B(predecessor.MODEL_INPUT), length=2),
        "announced_mulligan": predecessor.ListSpec(predecessor.B(predecessor.MODEL_INPUT), length=2),
    }
)
LONDON_ACTION_VARIANTS = {
    "choose_london_mulligan": predecessor.ObjectSpec({
        "action_kind": predecessor.E(["choose_london_mulligan"]),
        "actor": predecessor.Seat(),
        "mulligan_count": predecessor.I(predecessor.MODEL_INPUT, maximum=HAND_SIZE),
        "mulligan": predecessor.B(predecessor.MODEL_INPUT),
    }),
    "choose_london_bottom": predecessor.ObjectSpec({
        "action_kind": predecessor.E(["choose_london_bottom"]),
        "actor": predecessor.Seat(),
        "remaining": predecessor.I(predecessor.MODEL_INPUT, maximum=HAND_SIZE),
        "card": predecessor.CARD_STABLE_REF,
    }),
}
LONDON_LEGAL_ACTION_SPEC = predecessor.ObjectSpec(
    {
        "schema_version": predecessor.I(predecessor.OPERATIONAL_ONLY, maximum=0xFFFF_FFFF),
        "selected_index": predecessor.I(predecessor.OPERATIONAL_ONLY, maximum=0xFFFF_FFFF),
        "stable_id": predecessor.S(predecessor.FORBIDDEN),
        "semantic": predecessor.VariantSpec("action_kind", LONDON_ACTION_VARIANTS, predecessor.MODEL_INPUT),
        "display_text": predecessor.Opt(predecessor.S(predecessor.FORBIDDEN), predecessor.FORBIDDEN),
    }
)


def _digest(value: Any) -> str:
    raw = json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=False)
    return hashlib.sha256(raw.encode("utf-8")).hexdigest()


def feature_contract_fingerprint() -> str:
    return _digest({
        "registry_version": FEATURE_REGISTRY_VERSION,
        "observation_schema": 6,
        "predecessor_contract": predecessor.feature_contract_fingerprint(),
        "extensions": {
            "predecessor": "unchanged V4 extensions",
            "london_pregame": {"present": "only while a London opening is pending",
                               "seat_order": ["p0", "p1"], "spec": LONDON_PREGAME_SPEC.describe()},
            "projection_london_mulligans": "accepted only beside london_pregame and equal to it",
        },
        "legal_action": {"london": LONDON_LEGAL_ACTION_SPEC.describe(),
                         "refused": list(REFUSED_ACTION_KINDS)},
    })


def encoding_contract_fingerprint() -> str:
    return _digest({
        "encoding_version": ENCODING_CONTRACT_VERSION,
        "predecessor_encoding": predecessor.encoding_contract_fingerprint(),
        "dimensions": "unchanged V4",
        "in_game": "unchanged V4 tensors; numeric choice refused",
        "action_kinds": ACTION_KINDS,
        "engine_stages": ENGINE_STAGES,
        "pregame_state": {
            "london_pregame_stage_column": LONDON_PREGAME_STAGE_COLUMN,
            "self_has_lost_column": [SELF_HAS_LOST_COLUMN, "opponent_announced_mulligan_this_round"],
            "opponent_has_lost_column": [OPPONENT_HAS_LOST_COLUMN, "opponent_has_kept"],
            "values": [0.0, 1.0],
            "state_hash": "canonical V4 observation plus extensions.london_pregame in actor order",
        },
        "pregame_actions": {
            "kind_column": LONDON_OPENING_KIND_COLUMN,
            "announce": {"value_column": [MULLIGAN_VALUE_COLUMN, "mulligan_flag"],
                         "own_mulligans_column": [OWN_MULLIGANS_COLUMN, "div16"],
                         "opponent_mulligans_column": [OPPONENT_MULLIGANS_COLUMN, "div16"],
                         "opponent_kept_column": [OPPONENT_KEPT_COLUMN, "0_or_1_div16"],
                         "opponent_announced_column": [OPPONENT_ANNOUNCED_COLUMN, "0_or_1_div16"],
                         "card_reference": "none"},
            "bottom": {"remaining_column": [REMAINING_COLUMN, "div8"],
                       "own_mulligans_column": [OWN_MULLIGANS_COLUMN, "div16"],
                       "card_reference": "card role to the exact own hand object"},
            "other_columns": "V4 defaults (Normal cast mode, first cost kind, Decline)",
            "action_hash": "legal-action digest of the canonical London semantic",
        },
    })


def model_contract_fingerprint(schema: FeatureSchema) -> str:
    return _digest({"model_contract_version": MODEL_CONTRACT_VERSION,
                    "feature_schema": schema.__dict__})


def classification_registry() -> dict[str, str]:
    registry = dict(predecessor.classification_registry())
    registry["observation.extensions.london_pregame.*"] = predecessor.MODEL_INPUT
    registry["legal_action.semantic.choose_london_mulligan.*"] = predecessor.MODEL_INPUT
    registry["legal_action.semantic.choose_london_bottom.*"] = predecessor.MODEL_INPUT
    return registry


def _schema_v10(schema: FeatureSchema) -> FeatureSchema:
    return replace(schema, version=FEATURE_SCHEMA_VERSION, registry_version=FEATURE_REGISTRY_VERSION,
                   contract_digest=feature_contract_fingerprint(),
                   encoding_digest=encoding_contract_fingerprint())


def _split_observation(observation: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any] | None]:
    """The V4 observation and the London extension, if any."""
    if not isinstance(observation, dict) or type(observation.get("schema_version")) is not int:
        raise FeatureSchemaError("V10 observation requires integer schema_version")
    if observation["schema_version"] != 6:
        raise FeatureSchemaError("observation schema mismatch")
    extensions = observation.get("extensions")
    projection = observation.get("projection")
    if not isinstance(extensions, dict) or not isinstance(projection, dict):
        raise FeatureSchemaError("observation requires extensions and projection objects")
    london = extensions.get("london_pregame")
    public = projection.get("london_mulligans")
    base = observation
    if "london_pregame" in extensions:
        base = {**base, "extensions": {key: value for key, value in extensions.items() if key != "london_pregame"}}
    if "london_mulligans" in projection:
        if london is None:
            raise FeatureSchemaError("projection.london_mulligans requires extensions.london_pregame")
        base = {**base, "projection": {key: value for key, value in projection.items() if key != "london_mulligans"}}
    if london is None:
        return base, None
    LONDON_PREGAME_SPEC.validate(london, ("observation", "extensions", "london_pregame"))
    _validate_london_shape(london)
    if public is not None:
        if (public.get("phase") != london["phase"] or public.get("counts") != london["mulligans_taken"]
                or public.get("kept") != london["kept"] or set(public) != {"phase", "counts", "kept"}):
            raise FeatureSchemaError("projection.london_mulligans differs from extensions.london_pregame")
    return base, london


def _validate_london_shape(london: Any) -> None:
    if not isinstance(london, dict) or set(london) != {"phase", "mulligans_taken", "kept", "announced_mulligan"}:
        raise FeatureSchemaError("london_pregame requires exactly phase, mulligans_taken, kept and announced_mulligan")
    if london["phase"] not in LONDON_PHASES:
        raise FeatureSchemaError("london_pregame phase must be announce or bottom")
    counts = london["mulligans_taken"]
    if (not isinstance(counts, list) or len(counts) != 2
            or any(type(count) is not int or count < 0 or count > HAND_SIZE for count in counts)):
        raise FeatureSchemaError("london_pregame mulligans_taken requires two counts in 0..=7")
    for key in ("kept", "announced_mulligan"):
        flags = london[key]
        if not isinstance(flags, list) or len(flags) != 2 or any(type(flag) is not bool for flag in flags):
            raise FeatureSchemaError(f"london_pregame {key} requires two exact booleans")
    if any(kept and announced for kept, announced in zip(london["kept"], london["announced_mulligan"])):
        raise FeatureSchemaError("a seat that kept cannot announce a mulligan")


def _refuse_numeric_choice(legal_actions: Any) -> None:
    if not isinstance(legal_actions, list):
        raise FeatureSchemaError("legal actions must be a list")
    for action in legal_actions:
        semantic = action.get("semantic") if isinstance(action, dict) else None
        if isinstance(semantic, dict) and semantic.get("action_kind") in REFUSED_ACTION_KINDS:
            raise FeatureSchemaError("V10 refuses a numeric choice; column 15 is london_opening")


def _seat_index(seat: str) -> int:
    if seat not in predecessor.SEATS:
        raise FeatureSchemaError("seat must be p0 or p1")
    return predecessor.SEATS.index(seat)


def _cards_to_bottom(base: dict[str, Any], london: dict[str, Any], seat: int) -> int:
    if london["phase"] != "bottom":
        return 0
    hand = base["projection"]["hand_counts"][seat]
    return hand - (HAND_SIZE - london["mulligans_taken"][seat])


def _validate_london_decision(base: dict[str, Any], london: dict[str, Any], legal_actions: list[dict[str, Any]]) -> None:
    actor = base["acting_player"]
    me = _seat_index(actor)
    projection = base["projection"]
    if projection["turn"] != 1 or projection["phase"] != "untap" or projection["stack"]:
        raise FeatureSchemaError("a London decision precedes turn 1's first step")
    if any(status["has_lost"] for status in projection["player_status"]):
        raise FeatureSchemaError("no seat has lost at a London decision")
    if london["kept"][me]:
        raise FeatureSchemaError("a seat that kept has no London decision")
    if not legal_actions:
        raise FeatureSchemaError("a London decision requires its menu")
    for action in legal_actions:
        if not isinstance(action, dict) or set(action) - {"schema_version", "selected_index", "stable_id", "semantic", "display_text"}:
            raise FeatureSchemaError("legal action has unknown members")
        semantic = action.get("semantic")
        if not isinstance(semantic, dict) or semantic.get("action_kind") not in LONDON_ACTION_KINDS:
            raise FeatureSchemaError("a London decision admits only London rows")
        LONDON_LEGAL_ACTION_SPEC.validate(action, ("legal_action",))
        if semantic.get("actor") != actor:
            raise FeatureSchemaError("London row actor must be the acting player")
    kinds = {action["semantic"]["action_kind"] for action in legal_actions}
    own_hand = [card["stable"] for card in base["own_hand"]]
    if london["phase"] == "announce":
        if kinds != {"choose_london_mulligan"} or len(legal_actions) != 2:
            raise FeatureSchemaError("an announcement offers exactly keep and mulligan")
        if sorted(action["semantic"]["mulligan"] for action in legal_actions) != [False, True]:
            raise FeatureSchemaError("an announcement offers exactly keep and mulligan")
        for action in legal_actions:
            semantic = action["semantic"]
            if set(semantic) != {"action_kind", "actor", "mulligan_count", "mulligan"}:
                raise FeatureSchemaError("choose_london_mulligan has unknown members")
            if semantic["mulligan_count"] != london["mulligans_taken"][me] or semantic["mulligan_count"] >= HAND_SIZE:
                raise FeatureSchemaError("announcement count must equal the actor's mulligans taken")
        if projection["hand_counts"][me] != HAND_SIZE:
            raise FeatureSchemaError("an announcing seat holds seven cards")
    else:
        if kinds != {"choose_london_bottom"}:
            raise FeatureSchemaError("a bottom decision offers only bottom rows")
        remaining = _cards_to_bottom(base, london, me)
        if remaining <= 0:
            raise FeatureSchemaError("a bottom decision requires a card still to bottom")
        cards = []
        for action in legal_actions:
            semantic = action["semantic"]
            if set(semantic) != {"action_kind", "actor", "remaining", "card"}:
                raise FeatureSchemaError("choose_london_bottom has unknown members")
            if semantic["remaining"] != remaining:
                raise FeatureSchemaError("bottom remaining must equal the cards still to bottom")
            card = semantic["card"]
            if card not in own_hand or card["zone"] != "Hand" or card["owner"] != actor:
                raise FeatureSchemaError("a bottom row must reference an own hand card")
            cards.append(predecessor._stable_key(card))
        if sorted(cards) != sorted(predecessor._stable_key(card) for card in own_hand):
            raise FeatureSchemaError("a bottom decision offers every own hand card exactly once")


def canonical_london_pregame(london: dict[str, Any], actor: str) -> dict[str, Any]:
    """Actor-ordered canonical form: [self, opponent]."""
    me = _seat_index(actor)
    order = (me, 1 - me)
    return {
        "phase": london["phase"],
        "mulligans_taken": [london["mulligans_taken"][i] for i in order],
        "kept": [london["kept"][i] for i in order],
        "announced_mulligan": [london["announced_mulligan"][i] for i in order],
    }


def canonical_observation_v10(observation: dict[str, Any]) -> dict[str, Any]:
    base, london = _split_observation(observation)
    canonical = predecessor.canonical_observation_v7(base)
    if london is not None:
        canonical["extensions"]["london_pregame"] = canonical_london_pregame(london, base["acting_player"])
    return canonical


def _london_action_features(action: dict[str, Any], base: dict[str, Any], london: dict[str, Any],
                            registry: Any) -> tuple[list[float], list[list[float]], list[int], list[int]]:
    actor = base["acting_player"]
    me = _seat_index(actor)
    opponent = 1 - me
    semantic = action["semantic"]
    card = semantic.get("card")
    features = predecessor._one_hot("london_opening", ACTION_KINDS)
    features += predecessor._seat_features(semantic["actor"], actor)
    features += predecessor._card_ref_features(card, actor)
    features += [0.0] * len(predecessor.TARGET_KINDS) + predecessor._seat_features(None, actor)
    numeric = [0.0] * 22
    numeric[OWN_MULLIGANS_COLUMN - _NUMERIC_START] = predecessor._number(london["mulligans_taken"][me], 16.0)
    if semantic["action_kind"] == "choose_london_mulligan":
        numeric[OPPONENT_KEPT_COLUMN - _NUMERIC_START] = predecessor._number(int(london["kept"][opponent]), 16.0)
        numeric[OPPONENT_ANNOUNCED_COLUMN - _NUMERIC_START] = predecessor._number(
            int(london["announced_mulligan"][opponent]), 16.0)
        numeric[OPPONENT_MULLIGANS_COLUMN - _NUMERIC_START] = predecessor._number(london["mulligans_taken"][opponent], 16.0)
        numeric[MULLIGAN_VALUE_COLUMN - _NUMERIC_START] = predecessor._flag(semantic["mulligan"])
    else:
        numeric[REMAINING_COLUMN - _NUMERIC_START] = predecessor._number(semantic["remaining"], 8.0)
    features += numeric
    features += [0.0] + [0.0] * len(predecessor.MANA_COLORS) + [0.0] * len(predecessor.MANA_COLORS)
    features += predecessor._one_hot(predecessor.CAST_MODES[0], predecessor.CAST_MODES)
    features += predecessor._one_hot(predecessor.COST_ONE_HOT_KINDS_V7[0], predecessor.COST_ONE_HOT_KINDS_V7)
    features += predecessor._one_hot(predecessor.OPTIONAL_COST_CHOICES[0], predecessor.OPTIONAL_COST_CHOICES)
    canonical = predecessor._canonical_model_value(action, LONDON_LEGAL_ACTION_SPEC, ("legal_action",),
                                                   predecessor._CanonicalContext(actor))
    features += predecessor._digest_features("legal-action", canonical, ACTION_HASH_DIM)
    if len(features) != ACTION_FEATURE_DIM:
        raise FeatureSchemaError("London action row width drifted from the V4 contract")
    ref_rows: list[list[float]] = []
    ref_tokens: list[int] = []
    ref_nodes: list[int] = []
    if card is not None:
        row, token = predecessor._action_ref_row("card", 0, card, actor, 0)
        ref_rows.append(row)
        ref_tokens.append(token)
        ref_nodes.append(registry.resolve(card))
    return features, ref_rows, ref_tokens, ref_nodes


def encode_decision(observation: dict[str, Any], legal_actions: list[dict[str, Any]]) -> EncodedDecision:
    base, london = _split_observation(observation)
    if london is None:
        # In-game: the unchanged V4 encoder, so V7 tensors equal V4 tensors.
        _refuse_numeric_choice(legal_actions)
        encoded = predecessor.encode_decision(base, legal_actions)
        return replace(encoded, schema=_schema_v10(encoded.schema))
    predecessor.assert_observation_classified(base)
    _validate_london_decision(base, london, legal_actions)
    actor = base["acting_player"]
    me = _seat_index(actor)
    state = predecessor._state_features(base)
    state[SELF_HAS_LOST_COLUMN] = predecessor._flag(london["announced_mulligan"][1 - me])
    state[OPPONENT_HAS_LOST_COLUMN] = predecessor._flag(london["kept"][1 - me])
    state[LONDON_PREGAME_STAGE_COLUMN] = 1.0
    canonical = predecessor.canonical_observation_v7(base)
    canonical["extensions"]["london_pregame"] = canonical_london_pregame(london, actor)
    state[-STATE_HASH_DIM:] = predecessor._digest_features("observation-state", canonical, STATE_HASH_DIM)
    registry, object_rows, object_tokens, object_groups, object_node_ids, edge_rows, edge_sources, edge_targets = predecessor._objects(base)
    action_rows: list[list[float]] = []
    action_ref_rows: list[list[float]] = []
    action_ref_tokens: list[int] = []
    action_ref_indices: list[int] = []
    action_ref_nodes: list[int] = []
    for action_index, action in enumerate(legal_actions):
        row, ref_rows, ref_tokens, ref_nodes = _london_action_features(action, base, london, registry)
        action_rows.append(row)
        for row_ref, token_ref, node_ref in zip(ref_rows, ref_tokens, ref_nodes):
            action_ref_rows.append(row_ref)
            action_ref_tokens.append(token_ref)
            action_ref_indices.append(action_index)
            action_ref_nodes.append(node_ref)
    object_rows = predecessor._pad_rows(object_rows, OBJECT_FEATURE_DIM, "object_features")
    edge_rows = predecessor._pad_rows(edge_rows, EDGE_FEATURE_DIM, "edge_features")
    if not object_rows:
        object_rows = [[0.0] * OBJECT_FEATURE_DIM]
        object_tokens = [0]
        object_groups = [0]
        object_node_ids = [0]
    if not edge_rows:
        edge_rows_tensor = torch.empty((0, EDGE_FEATURE_DIM), dtype=torch.float32)
        edge_source_tensor = torch.empty((0,), dtype=torch.long)
        edge_target_tensor = torch.empty((0,), dtype=torch.long)
    else:
        edge_rows_tensor = torch.tensor(edge_rows, dtype=torch.float32)
        edge_source_tensor = torch.tensor(edge_sources, dtype=torch.long)
        edge_target_tensor = torch.tensor(edge_targets, dtype=torch.long)
    if action_ref_rows:
        action_ref_tensor = torch.tensor(action_ref_rows, dtype=torch.float32)
        action_ref_tokens_tensor = torch.tensor(action_ref_tokens, dtype=torch.long)
        action_ref_indices_tensor = torch.tensor(action_ref_indices, dtype=torch.long)
        action_ref_nodes_tensor = torch.tensor(action_ref_nodes, dtype=torch.long)
    else:
        action_ref_tensor = torch.empty((0, ACTION_REF_FEATURE_DIM), dtype=torch.float32)
        action_ref_tokens_tensor = torch.empty((0,), dtype=torch.long)
        action_ref_indices_tensor = torch.empty((0,), dtype=torch.long)
        action_ref_nodes_tensor = torch.empty((0,), dtype=torch.long)
    schema = _schema_v10(predecessor._schema(len(state), OBJECT_FEATURE_DIM, EDGE_FEATURE_DIM,
                                             ACTION_FEATURE_DIM, ACTION_REF_FEATURE_DIM))
    return EncodedDecision(
        schema=schema,
        state=torch.tensor(state, dtype=torch.float32),
        object_features=torch.tensor(object_rows, dtype=torch.float32),
        object_card_ids=torch.tensor(object_tokens, dtype=torch.long),
        object_groups=torch.tensor(object_groups, dtype=torch.long),
        object_node_ids=torch.tensor(object_node_ids, dtype=torch.long),
        edge_features=edge_rows_tensor,
        edge_source_indices=edge_source_tensor,
        edge_target_indices=edge_target_tensor,
        action_features=torch.tensor(action_rows, dtype=torch.float32),
        action_ref_features=action_ref_tensor,
        action_ref_card_ids=action_ref_tokens_tensor,
        action_ref_action_indices=action_ref_indices_tensor,
        action_ref_node_indices=action_ref_nodes_tensor,
    )
