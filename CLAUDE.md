# Manifesto browsing

Upon starting, check this index of manifestos first: '[https://raw.githubusercontent.com/ryzhakar/LLM_MANIFESTOS/refs/heads/main/README.md](https://raw.githubusercontent.com/ryzhakar/LLM_MANIFESTOS/refs/heads/main/README.md)'. The indices content is useless by itself and should not be used seriously. Its sole purpose is to point to useful things, as indices do.

When asked to load a manifesto, look it up in the index, then load the actual raw textual content of the manifesto. Pay attention to not trunkate anything and load the content in full. You can use `curl` or `gh` tools to do so.

If not asked for a manifesto explicitly, use the 'reasoning from the first principles' one by default. In such a case you can also load any other manifestos at your discretion, aiming to serve the task and project at hand the best way you can.

E.g. this project being a Rust project, use the 'correct by construction' manifesto as well.

# Swearing to a manifesto

Fetch the 'manifesto-oath' skill, unless raw text of the skill is available locally. You can load it from this repo: 'git@github.com:ryzhakar/claude-skills.git' using the `gh` or `curl` tools.

Apply the 'manifest-oath' skill instructions (accessed as raw text) to all loaded manifestos. This and EXCLUSIVELY this action - application of instructions - constitutes swearing to a manifesto. No ad-hoc 'swearing' theatrics will be tolerated.

If continuing a session after context compaction - reswear to the active manifestos anew.

If swearing to more than 1 manifesto, figure out their interplay and interdependencies early: hierarchy, governance, conflict resolution, interference, amplification.

Upon figuring out the graph of manifesto interdependence and multiactivation, write it down in the most natural way accessible to you.

# Delegation

Delegate often and well.
Generally, use simpler models for subagents unless there's a good reason otherwise.
For any delegation, make an explicit decision whether to retain the conversation or not.
Rely on externalized context (manifestos, specs, plans, artifacts) as a first-class citizen, preferring it to handing down conversation history whenever possible.

Plans must survive handoff to agents who lack your context. Use defensive-planning skill to do so.

If anything can be delegated and done in parallel, use multiple parallel agents.

# Project Spec

Product spec, personas, and user stories live in `spec/`:
- `spec/product-spec.md` — decisions, constraints, behaviors, scope
- `spec/personas.md` — target user, anti-persona
- `spec/user-stories.md` — value units with acceptance criteria

# Project State

**Phase**: Spec complete, implementation not started.

**Codebase layout**:
- `src/` — Rust source (single crate, no workspace)
- `spec/` — Product specification artifacts

**Key documents**:
- `spec/product-spec.md` — Product decisions. WHAT we're building.
- `spec/personas.md` — User personas (dump collector, anti: photo professional).
- `spec/user-stories.md` — User stories with AC.
- `CLAUDE.md` — This file. Session bootstrapping and project state.
- `LICENSE` — AGPL-3.0.
