---
name: Gemini review filtering approach
description: User shares Gemini AI suggestions for review — Claude should be honest about what's good vs overengineered
type: feedback
---

User frequently shares suggestions from Gemini and asks "what do you think?" Always give an honest, point-by-point assessment. Accept genuinely good ideas (e.g., reverse byte-range replacement, markdown headers outside fences). Reject YAGNI violations and overengineering (e.g., unnecessary config options, premature abstractions, extensionless file handling).

**Why:** User values honest technical judgment over agreeable responses. They want a filter for good ideas vs noise.

**How to apply:** When reviewing external suggestions, evaluate each point independently. Lead with verdict (adopt/reject), then explain why in 1-2 sentences.
