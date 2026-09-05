# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.28.0](https://github.com/HardMax71/amiss/compare/v0.27.0...v0.28.0) - 2026-09-05

### Changes

- Preserve resolved failure context and remove unreachable render path
- Produce and consume scanner reports through shared serde models
- Let report projections consume stored path types directly
- Keep portable write and flush failure checks with the emitter
- Emit typed report envelopes with serde and standard buffering
- Derive serialization for complete resolution outcomes
- Name resolved targets in resolver data
- Use shared report types for unavailable scanner inputs
- Serialize external plans and assessments directly to artifact bytes
- Preserve path-aware strict JSON refusals for evidence input
- Carry serialized external evidence through provider boundaries
- Use typed report fields for adoption and repair
- *(cli)* read typed human and JUnit reports
- Restrict machine-applicable fixes to text document paths
- *(cli)* read typed SARIF and Code Quality findings
- *(wire)* reuse canonical semantic output bytes
- *(cli)* require SARIF artifact for spanless locations
- *(cli)* serialize SARIF from borrowed structs
- *(cli)* clarify projection benchmark scope
- *(cli)* serialize Code Quality from borrowed structs
- *(scan)* share semantic evidence report provenance
- *(scan)* hash typed candidate identities
- *(report)* admit site-build defect evidence
- *(wire)* validate reports through typed model
- *(external)* derive assessment JSON
- *(external)* derive evidence JSON
- *(wire)* derive external plan JSON
- *(wire)* derive release manifest JSON
- *(controls)* derive organization floor JSON
- *(controls)* derive scanner policy JSON
- *(controls)* derive debt and waiver JSON
- *(wire)* derive execution constraint JSON
- *(wire)* derive trusted time JSON
- *(wire)* derive controls request JSON
- *(wire)* derive snapshot request JSON
- Use shared enum classifications in finding metadata
- Keep generated-report failures contextual and remove duplicate checks
- Check generated scanner reports in the contract drift gate
- Share projection difference data with the report model
- Use one projection reason vocabulary in producers and reports
- Produce shared evidence source counts directly
- Hash adoption facts from borrowed serde inputs
- Hash through the selected serde writer
- Stream produced finding keys and propagate evaluation errors
- Store finding key inputs as direct shared data
- Use shared finding scopes in the evaluator
- Stream candidate identities directly into their hasher
- Replace the observation identity encoder with shared serde models
- Stream typed index identities directly into SHA-256
- Isolate the process-wide fatal allocation measurement
- *(scan)* hash typed staged-index identities
- *(scan)* reuse report counter structs
- *(report)* share document classifications with the scanner
- *(scan)* share report decision types
- *(wire)* derive semantic evidence JSON
- reuse existing report field types in canonical order
- stream binary repository paths through serde
- Use report payload types directly without aliases
- Let report rows carry their stored serde types
- Let fact inputs borrow their key and evidence
- Derive serialization for resolution details
- Stream accepted finding keys in canonical field order
- Let serde enforce structural fact optionality
- Let report models carry validated producer paths directly
- Use typed analysis routes and one error-row builder
- Reject invalid feedback byte paths and zero location counts
- Serialize locale artifacts from borrowed payloads to bytes
- Serialize publication artifacts from borrowed payloads to bytes
- Carry canonical relation artifact bytes through the controller
- *(wire)* derive repository path serialization
- *(wire)* share resolver reasons with report models
- *(wire)* share snapshot identity models
- *(wire)* stream report findings canonically
- *(wire)* stream report observations canonically
- *(wire)* type report finding rows
- *(wire)* type report analysis rows
- *(wire)* type report provenance blocks
- *(wire)* exercise manifest closure boundaries
- *(controls)* bound floor resource limits
- *(wire)* derive evaluation request JSON
- *(wire)* derive candidate identity JSON
- *(wire)* derive record-set input JSON
- *(wire)* derive locale assessment JSON
- *(test)* insert locale pages by order
- *(wire)* derive locale evidence JSON
- *(wire)* derive locale plan JSON
- *(wire)* derive publication assessment JSON
- *(wire)* derive publication evidence JSON
- *(wire)* preserve publication parse diagnostics
- *(wire)* derive publication plan JSON
- *(wire)* derive relation assessment JSON
- *(wire)* derive relation plan JSON
- *(wire)* derive relation evidence JSON

## [0.27.0](https://github.com/HardMax71/amiss/compare/v0.26.0...v0.27.0) - 2026-09-01

### Changes

- *(wire)* assess relation projection transitions
- *(wire)* define relation projection evidence
- *(wire)* define relation audit plan
- *(locale)* assess page coverage offline
- *(locale)* define coverage evidence contract
- *(locale)* define coverage plan contract
- *(wire)* assess publication evidence offline
- *(wire)* define publication evidence contract
- *(wire)* define publication plan contract
- *(symbols)* attach Rust API records to docs
- *(cli)* produce canonical record-set templates
- *(cli)* bind self-asserted semantic templates
- *(semantic)* project exact record values
- *(scan)* meter projection work independently
- *(wire)* prepare projection selector primitives
- *(scan)* select exact named source regions
- *(scan)* evaluate policy-owned code projections
- *(wire)* declare code projection assertions
- *(site)* conform completed HTML fragment targets
- *(cli)* split invocation classification domains
- *(git)* [**breaking**] separate object protocols
- *(scan)* preserve exact tree projection semantics
- *(scan)* project exact repository sources
- *(semantic)* project complete record inventories
- *(scan)* compare complete inventory counts
- *(scan)* explain tree inventory differences
- *(scan)* compare exact tree path inventories
- *(scan)* ratchet projection assertion removals
- *(scan)* unify native resolution flow
- *(scan)* split semantic evidence domains
- *(md)* [**breaking**] keep corpus harvesters test-only
- *(service)* load relation registries
- *(gitlab)* bind relation status to policy jobs
- *(relations)* bind coordination identity
- *(relations)* freeze operator subject registry
- *(locale)* align inventories with exact products
- *(locale)* assess exact source lineage
- *(locale)* bind fallback provenance
- *(wire)* share bounded sidecar envelopes
- *(wire)* separate model domains
- *(wire)* separate strict JSON boundaries
- *(wire)* isolate control item grammar

## [0.26.0](https://github.com/HardMax71/amiss/compare/v0.25.0...v0.26.0) - 2026-08-28

### Changes

- *(cli)* keep invocation parser private
- address JUnit review findings
- add render-only JUnit projection
- Improve provider artifact feedback
- Make retry cache generational
- Enforce nonempty cached evidence
- Cache retry-local external evidence
- Author exact policy selectors
- Project canonical help grammar
- Remove stale window description
- Render complete human feedback
- Admit unattributed generated pages
- Bind semantic evidence to planned context
- Resolve generated site routes
- Report site-build routing defects
- Prove rendered mdBook navigation
- Recognize Bitbucket Data Center source URLs
- Recognize Bitbucket Cloud source URLs
- Delegate unavailable history to provider evidence
- Resolve locally available immutable forge references
- Preserve immutable forge reference identity
- Add reverse reference impact query
- Resolve fragment-changing redirects
- Consume terminal site redirects
- Consume candidate site-build routes
- Accept candidate-bound Intersphinx evidence
- Define the semantic evidence envelope
- add exact suffix policy selectors
- Render report projections without rescanning
- Keep Gitea format mismatches in repository scope
- Share semantic destination URI validation
- accept whitespace in rst directive paths
- resolve bounded local transclusions
- add exact relocation evidence
- Keep semantic evidence within strict JSON

## [0.25.0](https://github.com/HardMax71/amiss/compare/v0.24.0...v0.25.0) - 2026-08-21

### Changes

- Accelerate anchor identity collisions
- Harden JavaScript Unicode fixture decoding
- Streamline JSON storage and hashing
- Preserve truncated escape errors
- Streamline strict JSON parsing
- Streamline validated model construction
- Streamline JSON value narrowing
- Split bootstrap entrypoint
- Reject repeated manifest flags
- Declare errors with thiserror

## [0.24.0](https://github.com/HardMax71/amiss/compare/v0.23.0...v0.24.0) - 2026-08-20

### Changes

- Derive closed vocabulary spellings
- Narrow repository open errors
- Split human projections
- Streamline invocation parsing
- Derive wire control taxonomies
- Split wire report subsystems
- Compact immutable JSON values
- Borrow CLI report projections
- Use Result-based external command inputs
- Split pack entry decoder
- Split pack index decoder
- Reuse loose object inflater state
- Compact Git pack index lookups
- Reject duplicate Git tree names
- Split report analysis projection
- Split report identity projection
- Split anchor identity engine
- Split correlation engine
- Split scanner evaluation subsystems
- Split scanner policy subsystems
- Consolidate report taxonomies
- Split resolution subsystems
- Split scan pipeline orchestration
- Split evaluation by responsibility
- Derive report enum spellings
- Split report construction by responsibility
- Consume comparisons during report construction
- Reduce scan allocation churn
- Keep rendered observation identities coherent
- Stream observation identity hashes
- Resolve native paths in one buffer
- Avoid transient resolver allocations
- Count report payload once
- Correct case-neighbor measurement scope
- Make case-neighbor lookup allocation-free
- Aggregate report summaries in one pass
- Remove unreachable correlation duplicate check
- Index correlation component roots
- Split Markdown extraction subsystems
- Split evaluation request engine
- Split external plan engine
- Split external assessment engine
- Split control document models
- Encode digest wire form once

### Changes

- Expose validated provider-lane object identities directly and build their execution constraint once in shared fixtures
- Bind queued provider configuration directly into one shared repository-lane setup, removing parallel runtime assembly and automatic error conversions
- Share forge verification facts and operation deadlines directly across provider transports, removing their parallel result enums and deadline implementations
- Derive closed wire and scanner vocabulary parsing and projection with Strum, keep control trust typed through evaluation, and collapse adapter properties into one immutable metadata record
- Separate repository-opening failures from post-open Git access defects and project repository unavailability only at process boundaries
- Split the wire report subsystems and consolidate their taxonomies behind Strum spellings and immutable `FindingKind::metadata()`, replacing the public `as_str`, `fixed_phase`, `scope`, `evidence_class`, and `invariant_class` projection helpers
- Split scanner policy acquisition, effects, and floor enforcement into focused modules, and remove the unused public `floor_protected` batch helper
- Split scanner evaluation models and execution into focused modules, consolidate resolution-to-finding classification, and replace `Attribution::as_str` with Strum's `AsRef<str>` projection
- Split wire control taxonomies from document decoding, replace their public `as_str` helpers and `Profile::decode` with Strum projections and parsing, and share one typed enum decoder internally
- Split command-line argument collection from semantic classification, derive the closed verb and output-format parsers, and use canonical analysis-error identities and descriptions directly instead of mapping a duplicate invocation enum
- Split correlation data, component-graph construction, and comparison derivation into focused modules, derive correlation-reason spellings, and avoid temporary removed/added document vectors while detecting exact renames
- Separate the declarative renderer anchor contract from identity construction and duplicate resolution while preserving the public anchor API
- Separate report evaluation identity and control provenance projection from observation and finding projection, share repository-identity serialization, and preserve the public candidate-identity digest API
- Separate report observation, finding, and feedback projection from its public data model, project debt and waiver applications together with shared common rows, and inline the one-use location projection

## [0.23.0](https://github.com/HardMax71/amiss/compare/v0.22.0...v0.23.0) - 2026-08-17

### Changes

- streamline duplicate code and similarity gating
- Streamline validation and resolution flow
- Seal validated wire artifacts
- Strengthen core types and error flows
- Pin refreshed pack invariants
- Document pack ordering invariant
- Load only new packs on refresh
- Parse index paths once
- separate git resource meters
- Index Markdown reference definitions
- Bound anchor resolution work
- Bound report evidence lookups
- Bound source multiplicity aggregation
- correct protected floor regression
- linearize protected control checks
- reuse unchanged document scans
- reduce scan bookkeeping
- move observations through correlation
- enforce strict similarity edges
- Reuse Markdown traversal state
- Pin the self-closing unquoted value to its browser reading
- Keep an unquoted html attribute's slash
- Give a root-level orphan definition its within-node ordinal too
- Give each mined html tag its own address
- Strip the carriage return before the literal, label, and fence tests
- scan rst text blocks once
- scan rst interpreted text once
- Index external assessment destinations
- streamline wire member decoding

## [0.22.0](https://github.com/HardMax71/amiss/compare/v0.21.0...v0.22.0) - 2026-08-15

### Changes

- Share one copilot cache key across the lanes
- Cache the copilot toolcache in the mention and triage lanes
- State the classification rule, not the run's contents
- Name the undeclared identity beside the external count
- Hold the judge to its own contracts
- Judge the external plan against producer evidence
- Refuse an unknown identity host without a dialect
- Bound the plan's input read and keep refusal wording in the command
- Derive the external plan from a written report
- Tie the review cache to one copilot pin and save it before the agent runs
- Harden agent workflow bootstrap and tool setup
- Put the bench where the sandbox can see it and fail the reader closed
- Let cargo be the only parser of its own manifest
- Tie the bench to the manifest instead of its own file
- Declare the bench once and let everything read it
- Read the destinations raw HTML maintains
- Extract the definition no reference consumes
- Decode hex once and let the action word label itself
- Let the projection comment count both windows
- Give the backlog its own window
- List the carried backlog instead of only counting it
- Return the three narrowed excludes to the workspace law
- Link one test binary per crate, not one per file
- Move the unit tests in beside their modules
- Pin the reviewed div inputs against the foreign-tag consumption
- Close the three seams the second review round named
- Mine only what a renderer would follow
- Read the raw marker past a nested directive's indent
- Opaque means injected raw output in every dialect
- Probe unshaped introduced destinations with a pinned HTTPS client
- Treat cross-family ref shadowing as no fact
- Never refute a revision the commit route can still resolve
- Verify introduced forge destinations through the GitHub API
- Keep ambiguous gitlab paths unshaped and mark directory tails
- Attach the forge shape to plan rows on recognized hosts
- Zero the truncated pair the way the law already reads

## [0.21.0](https://github.com/HardMax71/amiss/compare/v0.20.0...v0.21.0) - 2026-08-10

### Changes

- Let only absence fall back past commondir
- Open the linked worktree through its pointer
- Carry the governed channel into rst and adoc
- Take the ordered lookup and the loud writes
- Reuse discovery's parse for anchor targets
- Freeze the wire at one
- Kill the carrier mutants inside the wire
- Say frozen where the row still said rolling

## [0.20.0](https://github.com/HardMax71/amiss/compare/v0.19.0...v0.20.0) - 2026-08-09

### Changes

- Pin document-over-tree precedence and the anchor row count
- Kill the binding mutants inside their package
- Let a policy include bind its grammar
- Answer the empty artifact even without an envelope
- Give the fileless finding a path the widget accepts
- Project the report as GitLab Code Quality

## [0.19.0](https://github.com/HardMax71/amiss/compare/v0.18.0...v0.19.0) - 2026-08-08

### Changes

- Wire the SARIF projection into code scanning
- Trim the authoring comments to their constraints
- Author through the evaluation's own line scanner
- Give amiss its claim authoring verb
- Answer the second review round on adoption
- Move the identity into the grammar and close the write race
- Prove the mint round trips into tolerance
- Mint the adoption debt from the evaluation
- Pin the index and the parent the repair trusts
- Give amiss its repair verb

## [0.18.0](https://github.com/HardMax71/amiss/compare/v0.17.0...v0.18.0) - 2026-08-08

### Changes

- Fold the scan tests into one linked suite
- Trim the summaries and project the fix sentences
- Turn the case neighbor into the path's fix
- Let a missed path name its one case neighbor
- Give the fix sentences the home the law names
- Fold the typography the renderers argue about
- Let the md battery read the path span it fills

## [0.17.0](https://github.com/HardMax71/amiss/compare/v0.16.0...v0.17.0) - 2026-08-07

### Changes

- Fold the artifact naming out of its twin
- Project the fix onto every machine surface
- Fold the claim fixture out of its twin shape
- Prove the claim on every surface the binary shows
- Make the command line answer below its surface
- Close the engine tail at its own boundaries
- Call the bucket prefix what it is
- A delta may name its base
- Craft what git refuses to write
- Every kind of object answers to its name
- Implementation is separate from tests
- Packaging strips what the guard never opens
- One loop reads for both platforms
- Answer the last two lints
- Answer the last two lints
- Give every dialect the fragment it can prove
- Turn the neighbor into the finding's fix
- Let the destination name its own bytes
- Let the neighbor step forward alone
- Give the missing anchor a place for its neighbor
- Hold the rewrite guard to its own clauses
- Let a broken claim carry its own repair
- Teach the wire a fix it does not yet emit
- Answer the scan group's twelve boundaries
- Pin the run id the gate carries through
- Give the tail's engine files their first direct answers
- Seed the unanswered claim, count the boundary you emit
- Group claims by key, and say the split out loud
- Let a value claim speak on the wire
- Carry every claim's answer to the report's door
- Teach the grammar its one claim kind
- Let a governed definition carry its decoded words
- Delete the guard that proof already held
- Keep the autolink hash out of the fragment gate
- Teach the dialect scanners their own clauses
- Break one opener at a time in the embedded lexer
- Hand the warden what the parser never writes
- Spell the near decode through the house idiom
- Spell the near decode and ratchet the twins down
- Trim the one clause that restated its fields
- Parse every lawful word, not just the spelled one
- The pointer answer survives every wrapper
- Every ceiling answers at its exact edge
- The derive is the declaration
- Walk the vocabulary to its end
- Let the enums enumerate themselves
- Close the worklist where the ledger says why
- Round-trip the bytes nobody ever wrote
- Let the artifact case reach the guard it tests
- Close the engine tail at its own boundaries
- Let the wrapper meet an engine that answers

## [0.16.0](https://github.com/HardMax71/amiss/compare/v0.15.0...v0.16.0) - 2026-07-31

### Changes

- Cut the launcher and let the platform hold the tags

## [0.15.0](https://github.com/HardMax71/amiss/compare/v0.14.0...v0.15.0) - 2026-07-31

### Changes

- Split the remaining four test monoliths
- Split the two test monoliths into directory tests
- Break the fixture monocultures the sweep named
- Read labels the way Docutils does

## [0.14.0](https://github.com/HardMax71/amiss/compare/v0.13.0...v0.14.0) - 2026-07-31

### Changes

- Resolve the references Sphinx actually writes
- Block only what the change introduced
- Speak SARIF where the scanners listen
- Build the engine where the arm runners live
- Count the notebooks and Org files this engine cannot read
- Mark reStructuredText includes as transclusion

## [0.13.0](https://github.com/HardMax71/amiss/compare/v0.12.0...v0.13.0) - 2026-07-30

### Changes

- update Cargo.toml dependencies
- Publish the identity Docutils gives a section
- Read reStructuredText documents, and say what that buys
- Publish the identity Asciidoctor gives a section
- Let the schema describe the observations AsciiDoc emits
- Read AsciiDoc documents, and declare what a tree cannot answer
- Count the markup this engine cannot read
- Ask the ignore file the repository already wrote
- Say the extraction vocabulary once, where both adapters can reach it
- update Cargo.lock dependencies

## [0.12.0](https://github.com/HardMax71/amiss/compare/v0.11.0...v0.12.0) - 2026-07-28

### Changes

- Trim the review's comments to one constraint line each
- Let every binary say which version it is, and the engine which build
- End the hung fixture with its parent, not with a clock

## [0.11.0](https://github.com/HardMax71/amiss/compare/v0.10.0...v0.11.0) - 2026-07-26

### Changes

- Say which spelling the destination is, and pin it in the schema
- Record an external destination instead of a finding about it

## [0.10.0](https://github.com/HardMax71/amiss/compare/v0.9.1...v0.10.0) - 2026-07-26

### Changes

- Read the identity an MDX heading declares in a comment
- Raise the report reservation past the largest documentation sets
- Skip the fixture trees and raise the changelog ceiling
- Read the identities a document declares outright
- Answer a router spelling instead of reporting it missing
- Pin what a documentation router serves
- Anchor the headings a document writes as raw HTML
- Answer a heading anchor instead of declining to
- Pin what ten renderers call a heading
- Publish the headings and anchors a renderer would read
- Stop saying the heading is left unchecked

## [0.9.1](https://github.com/HardMax71/amiss/compare/v0.9.0...v0.9.1) - 2026-07-25

### Changes

- Keep tests out of the published packages
- Declare each shared dependency version once
- Fold the controller crates into one workspace
- Fuzz controller authentication with generated keys
- Build fixture histories and worktrees without git too
- Document execution constraint provisioning
- Derive execution constraints from release trees

## [0.9.0](https://github.com/HardMax71/amiss/compare/v0.8.0...v0.9.0) - 2026-07-23

### Changes

- Fold licensing into the introduction
- Share required status validation
- Add checked UTC epoch conversion
- Add canonical commit identity builder
- Add provider-facing trust foundation
- Address provider controls review
- Streamline provider control paths
- Require controller-owned bootstrap outputs
- Seal bootstrap output files
- Carry bootstrap refusal classes at origin
- Make bootstrap termination coherent
- Emit closed bootstrap results
- Define bootstrap result records

## [0.8.0](https://github.com/HardMax71/amiss/compare/v0.7.0...v0.8.0) - 2026-07-20

### Changes

- add provider controller foundation
- bind the trusted-time statement's repository at sealed acceptance

## [0.7.0](https://github.com/HardMax71/amiss/compare/v0.6.2...v0.7.0) - 2026-07-19

### Changes

- normalize jq line endings
- group review feedback by target
- restore global finding locations

## [0.6.2](https://github.com/HardMax71/amiss/compare/v0.6.1...v0.6.2) - 2026-07-19

### Changes

- publish draft releases after smoke

## [0.6.1](https://github.com/HardMax71/amiss/compare/v0.6.0...v0.6.1) - 2026-07-19

### Changes

- Bump jsonschema from 0.46.10 to 0.47.0 in the cargo group
- smoke exact action before promotion
- address launch-readiness review
- close launch-readiness gaps

## [0.6.0](https://github.com/HardMax71/amiss/compare/v0.5.1...v0.6.0) - 2026-07-18

### Changes

- Bound embedded-code evaluation in-parse and add the Action watchdog
- Audit every chapter and fix the code links the site broke
- Make the project findable where machines look
- Drop a fired watchdog's report and tighten the charge surface
- Trim test doc comments to the constraints the code cannot show
- Cover the release-manifest laws the mutation run showed untested

## [0.5.1](https://github.com/HardMax71/amiss/compare/v0.5.0...v0.5.1) - 2026-07-18

### Changes

- End the narration on a closed pipe, never the verdict
- Host the action metadata at the repository root
- Meet the agents where they read: AGENTS.md, the Working with agents chapter, the Claude Code plugin marketplace and skill, and the gh-aw repair recipe

## [0.5.0](https://github.com/HardMax71/amiss/compare/v0.4.0...v0.5.0) - 2026-07-18

### Changes

- Flood annotations, explain the summary, index the book for agents
- Teach the grammar in refusals and finish the terse lanes
- Carry a fixed description on every finding and error row
- admit a floor limiting every resource
- Unify typed resolution contracts
- Document finding examples
- execute identity goldens
- execute published contract examples
- recover releases without forge defaults
- *(wire)* use rolling forge-neutral contracts
- align roadmap and release checks
- make fixture git commands hermetic
- enforce semantic vector contracts
- *(wire)* open execution-constraint identity
- *(scan)* split external control gate
- *(scan)* split forge resolver
- *(scan)* index exception targets
- *(scan)* index policy include lookups
- *(scan)* index discovered documents
- *(scan)* index correlation candidates
- exercise frontmatter suffix offset
- *(wire)* split trusted-time control

## [0.4.0](https://github.com/HardMax71/amiss/compare/v0.3.0...v0.4.0) - 2026-07-15

### Changes

- Speak the gitea family's typed forms, commit pins included
- Teach the resolver GitLab's spelling, and pin every dialect as data
- Lift the host gate and speak the third contract
- Carry the host in the identity, and compare identities whole
- Count the intent kinds by pointing at the schema, not a numeral
- Freeze the third contract's goldens from a forge-bearing run

## [0.3.0](https://github.com/HardMax71/amiss/compare/v0.2.0...v0.3.0) - 2026-07-15

### Changes

- Point the CLI crate's front doors at the real documentation
- Ship the engine as an action a workflow can pin

## [0.2.0](https://github.com/HardMax71/amiss/compare/v0.1.0...v0.2.0) - 2026-07-15

### Changes

- Admit the paths text cannot hold, and say so on the second wire
- Charge length before spelling, and disclose every unspellable index row
- Disclose the bytes of every name the report refuses
- Move the scaffolding every test rebuilt into the crate built for it
- Answer the README review: per-binary determinism, and the index mode named
- A README for the repository and one for each crate
- Meet the new digest generation halfway, and call what became a method
- *(deps)* bump the cargo group with 3 updates
- Say the quiet skip out loud when listing the store's names
- Thread the union path through the pipeline, gates intact
- Give the model both path forms the second contract names
- Report the line, not the byte offset, and mean it
- Freeze the goldens the engine itself emitted, and admit them whole
