---
name: writing-docs
description: Write or substantially revise pages under docs/. Not needed for typo or link-only fixes.
---

# Writing Conductor Docs

A useful page explains a constraint, the behavior that must stay true, and
where it is implemented. Document decisions and things the code cannot tell
you; link to source for anything that changes often.

- For a new page, pick its folder from [the docs index](../../../docs/index.md),
  read a neighbouring page for style, name it `NN-name.md`, and add it to the
  index with a one-line description.
- For an existing page, read the source behind each claim you change.
- Open with the problem or constraint. Short, plain prose; Title Case
  headings. Frontmatter is just `title` and `description`.
- Use repository links relative to the page. Link to official docs for
  external behavior.
- A rule lives in one place. Project-wide rules are in
  [AGENTS.md](../../../AGENTS.md); link instead of repeating them.

Done when the page matches the code you inspected and every relative link
resolves. State any claim you could not verify.
