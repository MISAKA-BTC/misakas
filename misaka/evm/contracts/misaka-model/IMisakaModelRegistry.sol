// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

/// @title IMisakaModelRegistry
/// @notice The window onto the PALW model registry — the rows of ADR-0056 / 0067 / 0075 /
///         0088 — served natively at `0x000000000000000000000000000000000000F010`
///         (ADR-0089 Decisions 1–2). Every function returns rows of
///         `fold(selected_parent(B))`: the state fold at the selected parent of the EVM
///         block `B` the call executes in. That is the one fold state that is a function of
///         `B`'s parents alone, so two nodes with the same store answer byte-identically and
///         no oracle stands between the chain and the caller.
///
///         VOCABULARY (ADR-0088 §3). A CLASS is a registered model family with a share and a
///         budget. A LINE is a model — a class, an owner, a name — and the FOUNDING line of a
///         class has the CLASS ID AS ITS LINE ID (every class has one; it exists implicitly
///         until something about it changes). A VERSION is one developer-signed publication on
///         a line: dense, monotone, 1-based (a "commit number"). The ROOTS IN FORCE of a class
///         are the union over its Active lines of the current root, the preview roots, and the
///         roots of superseded versions still inside their grace. A PAYLOAD is a bond's payout
///         payload — the 64-byte identity a bond pays to and the holder id of its positions.
///
///         ENCODING (binding): every 64-byte id — class, line, holder, payout payload, root,
///         hash, proposal id, evaluator id — is TWO `bytes32` words, HIGH half first, named
///         `<x>A` / `<x>B`.
///
///         UNKNOWN KEY ⇒ THE ZERO ROW, never a revert: the zero is itself a fact ("no such
///         row at this selected parent"). Malformed input (a calldata length the selector does
///         not accept) reverts and consumes the frame's gas. Below the `palw_model_evm` fence
///         the address is an empty account and every call returns empty data.
/// @dev    COUNTED vs DECLARED. `usage()` is the one measurement the chain makes (paid
///         inferences, counted by the fold in the arm that admits the claim). The hashes in
///         `version()` (runtime, dataset, training config, notes), every `evaluation()` and
///         every `proposal()` note are DECLARATIONS: the chain checks their length, records who
///         signed them, and never reads them. Nothing in the EVM should treat a score as a fact.
interface IMisakaModelRegistry {
    // ---------------------------------------------------------------------------------------
    // Classes (ADR-0056 / 0067 / 0075)
    // ---------------------------------------------------------------------------------------

    /// Number of class rows at the selected parent.
    function classCount() external view returns (uint256);

    /// The i-th class id, 0 ≤ i < classCount(). The enumeration order is the fold's own key
    /// order and is stable for one selected parent; it is NOT registration order.
    function classAt(uint256 i) external view returns (bytes32 classA, bytes32 classB);

    /// The class row.
    ///   status          0 Active, 1 Frozen, 2 Registered, 3 Dormant
    ///   sharePermille   the class's share, permille (0 when the class holds no share)
    ///   budgetBlocks    this epoch's ceiling on the class's produced blocks (ADR-0067)
    ///   canonicalLeaves the class's canonical leaf count — a quantity the chain owns
    ///   isBase          the floor class: no artifact, no line, no market
    ///   registrantA/B   the registrant bond's payout payload; zero for a genesis class
    ///   registeredDaa   DAA score of the registration
    function classRow(bytes32 classA, bytes32 classB)
        external
        view
        returns (
            uint8 status,
            uint16 sharePermille,
            uint64 budgetBlocks,
            uint64 canonicalLeaves,
            bool isBase,
            bytes32 registrantA,
            bytes32 registrantB,
            uint64 registeredDaa
        );

    /// Whether the class is certified (ADR-0075) on a lane: `lane` 0 = attempt, 1 = free-prompt.
    function certified(bytes32 classA, bytes32 classB, uint8 lane) external view returns (bool);

    /// Number of roots in force for the class at the selected parent (ADR-0088 Decision 3).
    function rootsInForceCount(bytes32 classA, bytes32 classB) external view returns (uint32);

    /// The i-th root in force, 0 ≤ i < rootsInForceCount(class).
    function rootInForceAt(bytes32 classA, bytes32 classB, uint32 i)
        external
        view
        returns (bytes32 rootA, bytes32 rootB);

    // ---------------------------------------------------------------------------------------
    // Lines (ADR-0088 Decisions 1, 6, 9)
    // ---------------------------------------------------------------------------------------

    /// Number of line rows at the selected parent. A founding line that nothing has touched
    /// has no row yet (ADR-0088 D1) and is not counted here; `line(classId)` still answers.
    function lineCount() external view returns (uint256);

    /// The i-th line id, 0 ≤ i < lineCount(). Fold key order, as for `classAt`.
    function lineAt(uint256 i) external view returns (bytes32 lineA, bytes32 lineB);

    /// Number of lines of the class (at most `PALW_MODEL_LINES_PER_CLASS_V1` = 64).
    function linesOfCount(bytes32 classA, bytes32 classB) external view returns (uint32);

    /// The i-th line of the class, 0 ≤ i < linesOfCount(class).
    function lineOfClassAt(bytes32 classA, bytes32 classB, uint32 i)
        external
        view
        returns (bytes32 lineA, bytes32 lineB);

    /// The line row.
    ///   classA/B            the class the line belongs to
    ///   ownerA/B            the owner bond's payout payload (zero: an unowned genesis line)
    ///   developerA/B        the developer bond's payload — the key that signs versions
    ///   maintainerA/B       the maintainer bond's payload
    ///   current             the current version number (1-based; 0 only if none is current)
    ///   versionsPublished   monotone count of versions ever published on the line
    ///   previewCount        previews held (at most `PALW_MODEL_PREVIEWS_V1` = 2)
    ///   contributorPermille the share of the owner's leg paid to an adopted contributor
    ///   status              0 Active, 1 Retired (the market is closed to buys)
    ///   nameHash            keccak256(name bytes); the name itself is not served
    function line(bytes32 lineA, bytes32 lineB)
        external
        view
        returns (
            bytes32 classA,
            bytes32 classB,
            bytes32 ownerA,
            bytes32 ownerB,
            bytes32 developerA,
            bytes32 developerB,
            bytes32 maintainerA,
            bytes32 maintainerB,
            uint32 current,
            uint32 versionsPublished,
            uint32 previewCount,
            uint16 contributorPermille,
            uint8 status,
            bytes32 nameHash
        );

    // ---------------------------------------------------------------------------------------
    // Versions (ADR-0088 Decisions 2, 4)
    // ---------------------------------------------------------------------------------------

    /// Version `n` (1-based) of the line. Only the last `PALW_MODEL_VERSION_HISTORY_V1` = 64
    /// versions of a line stay in state; an evicted `n` returns the zero row.
    ///   rootA/B        the artifact root this version puts in force
    ///   parent         the version it was derived from; 0 = none
    ///   adoptedA/B     the proposal id it adopted (ADR-0088 D7); zero = none
    ///   runtimeA/B     DECLARED runtime hash          — recorded, never read by the chain
    ///   datasetA/B     DECLARED dataset commitment    — recorded, never read by the chain
    ///   configA/B      DECLARED training-config hash  — recorded, never read by the chain
    ///   notesA/B       DECLARED notes hash            — recorded, never read by the chain
    ///   publishedDaa   DAA score of the publication
    ///   byA/B          the publishing developer bond's payload
    ///   status         0 Current, 1 Preview, 2 Superseded, 3 Withdrawn
    ///   untilDaa       for Superseded: the end of the grace during which the root stays in
    ///                  force (`publishedDaa` of the successor + `PALW_VERSION_GRACE_DAA_V1`);
    ///                  0 otherwise
    function version(bytes32 lineA, bytes32 lineB, uint32 n)
        external
        view
        returns (
            bytes32 rootA,
            bytes32 rootB,
            uint32 parent,
            bytes32 adoptedA,
            bytes32 adoptedB,
            bytes32 runtimeA,
            bytes32 runtimeB,
            bytes32 datasetA,
            bytes32 datasetB,
            bytes32 configA,
            bytes32 configB,
            bytes32 notesA,
            bytes32 notesB,
            uint64 publishedDaa,
            bytes32 byA,
            bytes32 byB,
            uint8 status,
            uint64 untilDaa
        );

    /// The fold's usage counters for version `n` — COUNTED, the one measurement the chain
    /// makes (ADR-0088 D4): accepted claims naming (or attributed to) this version's root.
    ///   attemptClaims  attempt-lane claims admitted
    ///   fpClaims       free-prompt-lane claims admitted
    ///   workLeaves     leaves of work those claims carried
    ///   firstUsedDaa   DAA score of the first such claim; 0 = never used
    ///   lastUsedDaa    DAA score of the latest such claim; 0 = never used
    /// A claim voided by the court is subtracted at the voiding.
    function usage(bytes32 lineA, bytes32 lineB, uint32 n)
        external
        view
        returns (uint64 attemptClaims, uint64 fpClaims, uint128 workLeaves, uint64 firstUsedDaa, uint64 lastUsedDaa);

    // ---------------------------------------------------------------------------------------
    // Evaluations (ADR-0088 Decision 5) — DECLARATIONS, from anyone, saying who declared them
    // ---------------------------------------------------------------------------------------

    /// Number of evaluations posted on version `n` (at most
    /// `PALW_MODEL_EVALUATIONS_PER_VERSION_V1` = 16; one per bond per version).
    function evaluationCount(bytes32 lineA, bytes32 lineB, uint32 n) external view returns (uint32);

    /// The i-th evaluation of version `n`. No consensus rule reads a score.
    ///   evaluatorA/B   the DECLARED evaluator id (a benchmark, a harness, a person — free text
    ///                  hashed by the poster)
    ///   scorePermille  the DECLARED score
    ///   reportA/B      the DECLARED report hash
    ///   byA/B          the posting bond's payload — the one thing the chain vouches for
    ///   postedDaa      DAA score of the posting
    ///   isLinesOwn     true when `by` is the line's developer or maintainer (the line's own
    ///                  word), false for a stranger's
    function evaluation(bytes32 lineA, bytes32 lineB, uint32 n, uint32 i)
        external
        view
        returns (
            bytes32 evaluatorA,
            bytes32 evaluatorB,
            uint32 scorePermille,
            bytes32 reportA,
            bytes32 reportB,
            bytes32 byA,
            bytes32 byB,
            uint64 postedDaa,
            bool isLinesOwn
        );

    // ---------------------------------------------------------------------------------------
    // Proposals (ADR-0088 Decision 7) — open research, recorded, paid when adopted
    // ---------------------------------------------------------------------------------------

    /// Number of proposals recorded on the line (at most `PALW_MODEL_PROPOSALS_PER_LINE_V1`
    /// = 32 open at once).
    function proposalCount(bytes32 lineA, bytes32 lineB) external view returns (uint32);

    /// The i-th proposal of the line.
    ///   idA/B      the proposal id, `H(line ‖ root ‖ by)`
    ///   rootA/B    the proposed artifact root
    ///   noteA/B    the DECLARED note hash
    ///   byA/B      the proposing bond's payload (paid `contributorPermille` of the owner's
    ///              leg while an adopting version is current)
    ///   postedDaa  DAA score of the posting
    ///   adoptedIn  the version number whose `adopted` names this proposal; 0 = not adopted
    function proposal(bytes32 lineA, bytes32 lineB, uint32 i)
        external
        view
        returns (
            bytes32 idA,
            bytes32 idB,
            bytes32 rootA,
            bytes32 rootB,
            bytes32 noteA,
            bytes32 noteB,
            bytes32 byA,
            bytes32 byB,
            uint64 postedDaa,
            uint32 adoptedIn
        );

    // ---------------------------------------------------------------------------------------
    // The facade family (ADR-0089 Decisions 1, 3) and the fold's clock
    // ---------------------------------------------------------------------------------------

    /// The line's MRC-20 facade address: `0x4d50 ‖ blake2b_512("misaka-evm/model-position-
    /// facade/v1" ‖ line_id)[..18]`. BLAKE2b is not available in Solidity, so THIS is how a
    /// contract learns a facade address. Zero for a line that does not exist.
    function facadeOf(bytes32 lineA, bytes32 lineB) external view returns (address);

    /// The line a facade address names; zero for an address that names no line (such an
    /// address behaves as an empty account even when it carries the `0x4d50` prefix).
    function lineOf(address facade) external view returns (bytes32 lineA, bytes32 lineB);

    /// The selected parent's DAA score — the fold's clock, the analogue of HyperEVM's
    /// `l1BlockNumber`. Every `*Daa` field in this interface is on this clock.
    function chainDaa() external view returns (uint64);
}
