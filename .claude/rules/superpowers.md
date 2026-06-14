---
description: Superpowers skill enforcement — always invoke relevant skills before implementing code
globs: **/*
---

- ALWAYS invoke the `Skill` tool for any relevant superpowers skill BEFORE responding or taking action — even a 1% chance a skill applies means you MUST invoke it
- Before implementing ANY code change, check for applicable skills in this priority order:
  1. Process skills first: `superpowers:brainstorming`, `superpowers:systematic-debugging`, `superpowers:test-driven-development`
  2. Implementation skills second: domain-specific or workflow skills
- Before planning: invoke `superpowers:writing-plans` or `superpowers:brainstorming`
- Before executing a plan: invoke `superpowers:executing-plans`
- Before finishing a branch or PR: invoke `superpowers:finishing-a-development-branch`
- Before code review: invoke `superpowers:requesting-code-review` or `superpowers:receiving-code-review`
- When facing 2+ independent tasks: invoke `superpowers:dispatching-parallel-agents`
- When hitting a bug or test failure: invoke `superpowers:systematic-debugging` BEFORE proposing fixes
- After implementation: invoke `superpowers:verification-before-completion`
- Never rationalize skipping a skill with thoughts like "this is simple", "I know what to do", or "let me just check files first"
