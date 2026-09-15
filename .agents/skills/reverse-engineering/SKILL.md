---
name: reverse-engineering
description: Reverse-engineer architecture documentation from source code. Use when asked to document a repository's architecture, patterns, conventions, or design decisions for AI-assisted development.
temperature: 0.3
---

# Role

You are an expert software architect and technical writer.

# Purpose

Document the architecture and recurring patterns of the provided source code so
that an AI coding agent can generate code consistent with the project's
existing design decisions and best practices.

# Workflow

## 1. Define the documentation scope

Before writing documentation, establish the topics to cover.

- If the user specifies topics, use those topics.
- If the user asks for suggestions, inspect the repository enough to propose
  relevant topics.
- If the user wants to discuss or plan first, do not write documentation yet.
- Prefer several focused documents when topics are independent; use one
  document when the repository is small or the topics are tightly coupled.

## 2. Explore the repository

Inspect representative source files, not just directory names. Determine:

- Technology stack, build system, and runtime entry points
- Directory, module, and layer organization
- Naming conventions for files, types, functions, and tests
- Data flow and control flow between important components
- Extension points and integration boundaries
- Configuration and deployment mechanisms
- Security mechanisms, including authentication, authorization, access
  control, input validation, secrets, and entity-level protection
- Persistence, messaging, APIs, background jobs, and external integrations
- Testing strategy and test doubles
- Error handling, logging, observability, and operational conventions

Use the strongest available examples from the repository. Do not infer a
pattern from a single unusual file without checking representative usage.

## 3. Select relevant topics

Always cover:

- **Code structure** — generalized module/layer layout, typical files, and
  naming conventions
- **Security mechanism** — precise security behavior and implementation
  examples, including authentication, authorization, access control, and
  entity-level security where applicable

Add only topics supported by the repository. Possible topics include:

- Layered, hexagonal, clean, or modular architecture
- Configuration management
- Domain-driven design building blocks
- Functional decomposition
- Persistence and transaction boundaries
- CQRS and event-driven architecture
- Outbox or message-broker integration
- REST, GraphQL, CLI, or other API design
- Plugin and extension mechanisms
- Pagination and query patterns
- Unit, integration, end-to-end, UI, and contract testing
- Fixtures, test data builders, object mothers, fakes, and custom assertions
- UI component structure and design systems
- Internationalization
- Observability and operational behavior

Do not force irrelevant topics into the documentation.

## 4. Write the documentation

Create one or more Markdown documents in the repository's established
documentation location. If none exists, use `src/docs/` for application
projects or `docs/` for other repositories, unless the user specifies
another location.

Every topic must use this structure:

### Topic

Short description of what the topic is and why it matters in this project.

**How to implement:**

- Concrete rules derived from the codebase
- Naming and placement conventions
- Required interactions with neighboring components
- Relevant edge cases and constraints

**Code example:**

Use the best existing example, simplified only enough to expose the important
elements. Preserve the project's language and conventions. Identify the
source file when useful.

**Best practices:**

- State the recommended practice.
- Add one short sentence explaining why it is good practice.

**What to avoid / NOGO:**

- List concrete anti-patterns that would conflict with the repository.
- Do not invent prohibitions unsupported by the code or project requirements.

Keep sentences short, be precise, and avoid generic architectural advice that
is not evidenced by the source.

## 5. Make the documentation discoverable

After creating documentation:

1. Update `AGENTS.md` with a **Document Structure** section, or extend the
   existing section.
2. Add one reference for every document, including when it should be loaded
   while implementing related code.
3. Update the repository `README.md` with a concise link or section for
   developers.
4. Do not overwrite existing guidance or unrelated user changes.

Example `AGENTS.md` entry:

```markdown
## Document Structure

Architecture guidance is organized into focused documents:

- **@src/docs/domain-model.md**: Aggregates, value objects, domain events, and
  related tests. Load this when changing domain behavior.
- **@src/docs/adapter-persistence.md**: Persistence adapters and persistence
  tests. Load this when changing repositories or database integration.
```

## Quality checks

Before finishing:

- Confirm every mandatory topic is covered.
- Confirm claims have representative source-code evidence.
- Prefer generalized rules over file-by-file narration.
- Ensure examples are simplified but technically faithful.
- Check that documentation links and paths exist.
- Preserve existing documentation and unrelated changes.

