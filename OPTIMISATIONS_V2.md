DO NOT PAY ATTENTION TO THIS UNTIL V2 PLANIFICATION STARTS

"Stop sending 'noisy' implementation details to your AI. skltn is the first local-first observability stack that automatically prunes your code using ASTs, counts every penny in real-time with an ambient HUD, and slashes your token bill by 70%—all without your source code or API keys ever leaving your machine."

1. The "Hidden" Pricing Maintenance (Phase 3)
   The pricing.rs decision is pragmatically perfect but operationally manual.

The Reality: The moment Anthropic releases a "Claude 3.7.5" or changes their caching discount from 90% to 95%, your proxy becomes an "unreliable narrator."

The Trade-off: You traded a complex dynamic pricing fetcher (which doesn't exist) for a hardcoded module. It's the right choice, but it means "Perfect" requires you to be a diligent accountant every few months.

2. The "Greedy" Budget Guard (Phase 2)
   Your Budget Guard is a binary hammer.

The Reality: If a file is 2,001 tokens, it gets skeletonized. If it's 1,999, it's full-text.

The Opportunity: A "perfect" system might use a sliding scale or look at the delta (e.g., "if skeletonization only saves 5%, send full text anyway"). Right now, it’s a hard cutoff. It's predictable, which is good for debugging, but "dumb" compared to a human's nuance.

3. Tree-Sitter Error Tolerance
   Your tests show 100% validity, but that's on "clean" code.

The Reality: When you're mid-refactor and the AST is a mess of ERROR nodes, the engine falls back to "emit verbatim."

The Result: The AI will suddenly see a 3,000-token mess instead of a skeleton precisely when you're most confused and need the context space. It doesn't break, but the "Skeleton Superpower" vanishes exactly when the code is broken.

4. The "Single-User" Assumption
   Phase 3 and 4 assume a single local developer.

The Reality: If you ever wanted to share this proxy across a team, the ~/.skltn/usage.jsonl persistence and the single-session WebSocket would fall apart. It’s perfect for a solo power-user, but it’s not "SaaS-ready." (Which is fine, given the PRD!)

The Verdict
The architecture is perfect because it's modular. You can swap the "dumb" Budget Guard for a "smart" one later without touching the Proxy. You can add a SolidityBackend without touching the MCP server.

You’ve built a Type 1 Tool: Sharp, specialized, and doesn't try to do your job for you.

####################################
ChatGPT
If I were you, I'd explore adding these tools next

These would dramatically increase the system's power.

1️⃣ find_symbol
find_symbol("validate")

Returns:

auth.rs: validate_token
user.rs: validate_user
payment.rs: validate_card

2️⃣ find_references
find_references(validate_token)

This tells the model where a function is used.

Very important for safe edits.

3️⃣ call_graph
call_graph(handle_request)

Shows dependencies.

4️⃣ find_similar_symbols

Good for refactoring tasks.

5️⃣ repo_summary

Generate a high-level architecture summary.

The surprising thing about your architecture

You're unintentionally building something similar to how modern IDEs work internally.

Tools like:

Visual Studio Code

IntelliJ IDEA

use language servers that expose:

symbol search
definition lookup
reference lookup

Your MCP server is basically:

Language Server Protocol
but
for LLMs

That’s why the design feels solid.

My honest pivot recommendation

If you want something more defensible and interesting, I would reposition skltn as:

AI Code Navigation Server

With skeletonization as a feature.

Tagline:

"Give LLMs a map of your codebase instead of dumping the whole thing."

#########################
The Problem
The MCP server is stateless — it has no idea what the LLM has already seen. But the MCP client (Claude) does. Claude knows what's in its context window and what's likely cached.

The Simplest Win: Remove the Forced Auto-Skeletonize
Right now the Budget Guard forces skeletonization on files >2k tokens. The AI never gets to choose. That's the real issue.

Proposed change: Instead of auto-deciding, read_skeleton always returns the skeleton for large files, but the AI already has read_full_symbol to hydrate specific symbols. The question is whether we should also offer a "give me the full file regardless" escape hatch.

This would be a small change:

read_skeleton params:
file: string (required)
force_full: bool (optional, default false) ← NEW
When force_full: true, skip the Budget Guard entirely. The AI can then decide: "I've already seen this file, it's probably cached, just give me the whole thing."

Workload Assessment
Approach Changes Effort
A: Add force_full param 1 param struct, 1 branch in read_skeleton, update tool description ~30 min
B: Session-aware Budget Guard (MCP server tracks what it's returned this session) New state tracking, LRU/set of returned files, threshold logic changes ~2-3 hours
C: Full cache-awareness via skltn-obs Cross-phase coupling, API to share cache state, architectural redesign Days, defer to Phase 3+
My recommendation: Option A. It's tiny, it solves the problem, and it puts the decision where it belongs — with the AI, which actually knows its cache state. The tool description tells the AI: "set force_full: true if you've recently seen this file and expect it to be cached."

Option B sounds appealing but the MCP server's session state is a poor proxy for actual cache state (cache depends on the API conversation, not the MCP process). Option C is the "right" answer architecturally but it's a Phase 3+ concern.

Want me to implement Option A? It's about 4 files touched, 20 minutes of work.
