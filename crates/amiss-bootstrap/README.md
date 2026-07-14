# amiss-bootstrap

The trusted CI wrapper for Amiss. It validates a pinned action tree as plain data, checks
the release manifest, the runtime closure, and the engine digest, and only then launches the
verified engine with a cleared environment, fixed arguments, and a wall-clock timeout. The
one crate in the workspace allowed to start a process.

Part of [Amiss](https://hardmax71.github.io/amiss/).
