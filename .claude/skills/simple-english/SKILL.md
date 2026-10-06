---
name: simple-english
description: Write user-facing text of this repository in ASD-STE100 Simplified Technical English. Use it before you write or rewrite README.md, a document of docs/, an error message, UI text, a commit body, issue text, or pull request text.
---

# Simple English

This skill holds the writing rules of this repository. `AGENTS.md` names this skill and
holds no copy of these rules.

## Where the rules apply

Apply the rules to text that a reader outside this conversation sees:

- `README.md` and the documents of `docs/`
- An error message and the text of the user interface
- A commit body
- Issue text and pull request text

Do not apply the rules to a chat reply or to a working note. `AGENTS.md` gives the style
of a chat reply.

## The rules

1. Use a maximum of 20 words in an instruction. Use a maximum of 25 words in a
   description.
2. Use the active voice.
3. Use the imperative for a step.
4. Put the condition before the command.
5. Use one word for one concept. Do not name one thing with two words.
6. Keep the articles and the full grammar. A short sentence is not a fragment.
7. Do not use "should", "may", "however", or "therefore".
8. Write one topic in one paragraph.
9. Name the thing that failed. An error message names the input that failed.

## Examples

| Do not write | Write |
| --- | --- |
| The file should be closed by the caller. | Close the file. |
| The read failed. | The read of the row `euw1:12` failed. |
| Run the test, however the lint runs first. | Run the lint. Then run the test. |
| The store, the cache, and the repository hold the row. | The store holds the row. |
| The command is executed if the flag is set. | If the flag is set, run the command. |

## The check

Before you send the text, read each sentence one time:

1. Count the words. An instruction of more than 20 words needs two sentences.
2. Find the subject of each verb. A sentence without a subject needs the active voice.
3. Search the text for "should", "may", "however", and "therefore". Remove each one.
4. Search the text for two words of one concept. Keep one word.
