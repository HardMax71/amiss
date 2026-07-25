# Every accepted delivery carries a replay lifetime

Replay suppression needs an end. Keeping every delivery identity forever is a storage leak;
forgetting one too early reopens a signed request that is still valid, which is a replay hole
rather than a tidy-up.

Trusted ingress therefore stamps each accepted delivery with a lifetime at admission. A
request authenticated by exact body, or by a replay-only scheme, is permanent, because
nothing in it says when it stops being valid. A request carrying an authenticated ID and
issue time gets a fixed end derived from the controller's signed-age and queue ceilings. A
route may narrow freshness beyond that, and cannot extend the lifetime already stored.

Ingress and the signature schemes are [`controller/src/ingress.rs`](https://github.com/HardMax71/amiss/blob/main/controller/src/ingress.rs) and
[`controller/src/webhook/`](https://github.com/HardMax71/amiss/tree/main/controller/src/webhook). Landed with the provider trust foundation in
[#103](https://github.com/HardMax71/amiss/pull/103).
