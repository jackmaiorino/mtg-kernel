# Destroy nonland permanent and replace it with a Human

Stroke of Midnight is prepared without registration. Pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40` defines a 2W instant that destroys
target nonland permanent. Its controller creates a 1/1 white Human creature
token. HumanToken.java supplies the dependency's creature/Human, white, 1/1
and no-keyword definition. Its frozen FDN token identity is
`06af3d5a-7b38-42f6-822f-da4e04a7f265`, required by Stroke of Midnight.

The spell reuses NonlandPermanent and DestroyObjectThenCreateTokens. The
existing interpreter reads the target's current controller before destroying
it and creates the token even when indestructible prevents destruction.
Ordinary spell target validation rejects stale or otherwise illegal targets
before running either effect. Token creation uses ordinary entry events.

The Human dependency must be registered alongside the spell, never as a
deckable card. Neither definition is admitted here. Four unregistered gameplay
groups cover metadata, colored cost, own/opposing nonland types, current
controller changes, indestructible, entry triggers, illegal targets, blinking
and saved-game replay at both seats. Source formatting and diff checks are
preparation; compilation, gameplay and catalog/profile acceptance remain
pending. No evaluation, training or strength result is claimed.
