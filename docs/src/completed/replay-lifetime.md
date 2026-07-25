# Every accepted delivery carries a replay lifetime

Replay suppression needs an end. Keep every delivery identity forever and the record grows
without bound. Forget one too early and a signed request that is still valid becomes replayable,
which is a security hole wearing the costume of a cleanup job. The question is who decides when
forgetting is safe, and the answer cannot be whoever is asking.

Trusted ingress stamps each accepted delivery with a lifetime at admission, derived from what
the request itself can prove. A delivery authenticated by exact body, or by a scheme that only
proves replay identity, is permanent, because nothing in it says when it stops being valid and
guessing an end would be inventing a fact. A delivery carrying an authenticated ID and issue
time gets a fixed end computed from the controller's signed-age and queue ceilings, so the end
comes from configuration the operator set rather than from the sender.

A route may narrow freshness beyond that. A route may not extend the lifetime already stored.
That asymmetry is the whole point: the strict direction is always available, the permissive one
never is, so no per-route setting can quietly reopen a replay window the controller closed.

The Gitea family is the concrete case. Its native webhook signature covers the body and nothing
else, with no timestamp anywhere in the delivery, so its replay markers are permanent and the
provider page says so rather than implying a window that does not exist.

Landed with the provider trust foundation in [#103](https://github.com/hardmax71/amiss/pull/103) and carried through controller
execution in [#105](https://github.com/hardmax71/amiss/pull/105). Ingress is
[`controller/src/ingress.rs`](https://github.com/hardmax71/amiss/blob/main/controller/src/ingress.rs) and the signature schemes are
[`controller/src/webhook/`](https://github.com/hardmax71/amiss/tree/main/controller/src/webhook).
