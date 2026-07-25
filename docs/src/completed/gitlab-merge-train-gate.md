# The GitLab gate refuses anything but the exact saved pass

A synchronous endpoint that returns success on anything other than a proven pass is a gate in
name only, since the job it answers is what lets the train merge.

Refresh requires the configured merge method, exactly two train parents, an active policy job,
a protected target branch with no push or bypass path, and merge-train enforcement for all
users. Any of those missing means the shape being gated is not the shape that was verified.
The endpoint then lets only the exact saved pass return success. Block, unavailable,
duplicate, expired, replayed, or changed state all keep the policy job failed.

The rules are [`controller/gitlab/src/live/refresh.rs`](https://github.com/HardMax71/amiss/blob/main/controller/gitlab/src/live/refresh.rs). Completed in
[#107](https://github.com/HardMax71/amiss/pull/107).
