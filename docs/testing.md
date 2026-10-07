# Testing

This document gives the test rules of this repository.

## The location of a test

- Put a unit test in the same file as the code. Use a `#[cfg(test)] mod tests` block.
- Put an integration test in the `tests/` folder of its crate.

## The network

A test does not use the network. A test does not open a socket to a different computer.

If a test needs data from a network peer, use a recorded response. A recorded response is a fixture.

## Fixtures

- Put a fixture in the `tests/fixtures/` folder of its crate.
- Give a fixture a name that tells its contents. Example: `client-input-jump.bin`.
- If a fixture is a recorded message, write the tool and the date of the record in a comment of the test.

A test that connects a host and a client in one process is permitted. Use the loopback address or a channel in memory.

## Bevy systems

To test a system, create an `App` with `MinimalPlugins`. Add only the plugins and the systems that the test needs. Then call `app.update()`.

Do not open a window in a test. Do not use the GPU in a test.

## Coverage

Each pure function of the gameplay logic has a unit test. A pure function reads only its arguments and changes no state.

Examples of gameplay logic are the stamina rule, the cloud sea depth, and the payout rule.

A system that only moves data between components does not need a unit test. Test it with an integration test.

## Run the tests

`docs/readiness.md` gives the test command.
