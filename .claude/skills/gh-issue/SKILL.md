---
name: gh-issue
description: A skill to be used when prompted only. Directs the agent to look up GitHub issues and resolve them accordingly.
---

# GitHub Issue

This skill is loaded by the user to resolve GitHub issues the user logs while testing.
The user may prompt you to address a specific issue by referencing the number. They may also make a more widespread request for issues to be resolved. If the user does not specify anything, assume they want all open issues fixed.

## When to use

ONLY use this skill when the user prompts you.

## Retrieving issues

**A specific issue** (user gave a number). One call, everything needed:

```sh
gh issue view 110 --json number,title,body,labels,comments
```

## Taking action

When addressing multiple issues the agent can spawn multiple subagents to address each issue or specific actions as is seen fit by the agent. The goal is token efficiency first and time efficiency as a close second. For very heavy reading tasks the agent may want to use a Sonnet class sub-agent instead of the larger Opus. Writing operations should always be performed by an Opus class sub-agent.
Invoking this skill is explicit permission to use subagents for this task — spawn them without asking the user first, overriding any default that says otherwise.

## Limits

Never create new issues if not directly prompted by the user. Do not change existing issues labels or state unless directly prompted by the user. You can leave comments on issues always signing `CLAUDE` so the user can distinguish between their own comments, this is especially relevant for investigations. Do not overdo this or write very lengthy comments.
Do not make commits or create branches unless directed to, perform actions on the local environment.