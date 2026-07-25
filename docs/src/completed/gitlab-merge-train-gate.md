# The GitLab gate refuses anything but the exact saved pass

The policy job is a synchronous endpoint: it asks the service a question and merges on the
answer. Anything other than a proven pass returning success turns the whole lane into
decoration.

Refresh requires the configured merge method, exactly two train parents, an active policy job,
a protected target branch with no push or bypass path, and merge-train enforcement for all
users. Each is a way the shape being gated could differ from the shape that was verified. Two
train parents in particular is what makes the train result the thing that merges rather than
some other commit that happens to be nearby.

Then the endpoint refuses everything except the exact saved pass. Block, unavailable,
duplicate, expired, replayed, and changed state all keep the policy job failed. There is no
"probably fine" state, and no path where a missing answer reads as an affirmative one.

Completed in [#107](https://github.com/hardmax71/amiss/pull/107). The rules are
[`controller/gitlab/src/live/refresh.rs`](https://github.com/hardmax71/amiss/blob/main/controller/gitlab/src/live/refresh.rs).
