# amiss

The Amiss engine and command line. It compares documentation against the repository tree at
a base commit and either a candidate commit or the staged index, entirely in-process: no
subprocesses, no network, no writes, and the same input through the same binary always
produces the same report bytes.

Install with `cargo install amiss`; the whole command grammar and everything else is in the
[documentation](https://hardmax71.github.io/amiss/).
