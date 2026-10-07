# Stacked pull requests

This document gives the workflow for a feature of more than one pull request.

## When to use a stack

Use a stack for a feature. Each pull request of the stack contains one cohesive change.

A change that is not part of a feature targets `master` directly. A CI change, a document, and a dependency update are examples.

## Create a stack

1. Create the first branch from `master`.
2. Open the first pull request to `master`.
3. Create the next branch from the first branch.
4. Open the next pull request to the first branch.
5. Do steps 3 and 4 again for each subsequent branch.

`docs/branch-naming.md` gives the names of the branches.

## The pull request text

In the text of each pull request, give the list of all pull requests of the stack. Mark the current pull request in the list.

## Change a branch in the middle of a stack

1. Make the change on the branch.
2. Rebase each subsequent branch on the branch before it.
3. Push each rebased branch with `git push --force-with-lease`.

Do not use `git push --force` without the lease.

## Merge a stack

1. Merge the first pull request into `master`.
2. Rebase the next branch on `master`.
3. Change the base of the next pull request to `master`.
4. Merge the next pull request.
5. Do steps 2 to 4 again for each subsequent pull request.

Merge the pull requests in the order of the stack. Do not merge a pull request before the pull request below it.
