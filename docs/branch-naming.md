# Branch naming

This document gives the names of the branches of this repository.

## The form

Use the form `<type>/<topic>`.

- The `<type>` is a type of Conventional Commits. The list below gives each type.
- The `<topic>` is two to five words in lowercase. A hyphen joins the words.

Example: `feat/pigeon-stamina`.

## The types

| Type | Use |
| --- | --- |
| `feat` | A new behavior of the game |
| `fix` | A correction of a bug |
| `refactor` | A change of structure without a change of behavior |
| `docs` | A change to a document only |
| `test` | A change to a test only |
| `ci` | A change to the CI configuration |
| `build` | A change to a dependency or to the build configuration |
| `chore` | A change that has no other type |

## A branch of a stack

If a branch is part of a stack, add the position of the branch in the stack after the type.

Use the form `<type>/<feature>-<position>-<topic>`.

Example: `feat/pigeon-controller-1-movement`, then `feat/pigeon-controller-2-flight`.

`docs/stacked-pull-requests.md` gives the workflow of a stack.
