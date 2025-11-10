# via

Issue commands across multiple interactive CLI sessions.

## Overview

`via` lets you start named interactive sessions (REPLs, shells, etc.) and control them from any terminal. It's mainly just a wrapper for [teetty](https://github.com/mitsuhiko/teetty) but adds some conveniences making interactions with a command prompt more robust and convenient.


## Disclaimer

This is tool is primarily for my own personal use with LLM coding assistants like Claude and Codex. I do not expect to spend much time maintaining or upgrading `via`, but if someone wants to volunteer to take over, feel free to drop me a message.

Also, I'm not a Rust developer and leaned heavily on Claude to port my original shell script. I've mainly selected Rust because `teetty` is coded in Rust and I didn't want to add any dependencies on top of that tool.

## Example

Start a Nix REPL session in one terminal:
```bash
$ via nix run -- nix repl
...
```

Start a python session in another terminal
```
$ via python run -- python
...
```

Interact both from yet another terminal
```bash
$ # Check what sessions are running
$ via
Session  Prompt Line  Working Directory  Command
nix      nix-repl>    /home/me/projects  nix repl
python   >>>          /home/me/projects  python

$ # Send a command and get the result
$ via nix 'nix-repl>' '1 + 1'
nix-repl> 1 + 1
2

$ # Pipe commands into the repl
$ echo 'print("hello")' | via python '>>>'
>>> print("hello")
hello

```

This enables scripting interactive CLIs that weren't designed for automation.

## Installation

Only nix based installation is supported for right now and I assume you are familiar enough with it to not need help.

Assuming you have nix flakes enabled you can try it out with

```
$ nix shell github:rehno-lindeque/via
$ via --help
```


### Usage

```
via [--simple]                        # list sessions
via help                              # show help
via <session> run -- <cmd> ...        # start a new named session
via <session> 'PROMPT>' [line...]     # write input and read output in one command
```

Low-level commands:

```
via <session> write [line...]         # write to session stdin
via <session> tail -n N               # tail last N lines
via <session> tail -f [-n N]          # follow output in real-time
via <session> tail --since 'PROMPT>'  # tail since last PROMPT
via <session> tail --delim 'PROMPT>'  # last stanza since PROMPT
via <session> path                    # show session directory path
```

## License

Apache-2.0 - See LICENSE file for details
