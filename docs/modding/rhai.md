# Rhai in ten minutes

Game rules are written in [Rhai](https://rhai.rs), a small scripting
language. You don't need Rust or any compiler: you write a `.rhai` text
file, `bri-addon-check` reads it, and the host runs it. If you have written
JavaScript, Lua or TorqueScript, most of it will look familiar. This page
covers the parts rules use; [Game rules](rules.md) lists what a rule can
call, and the [Rhai book](https://rhai.rs/book/) has the whole language.

**Your editor.** Any text editor works. Visual Studio Code colours Rhai
with the Rhai project's own extension (search the extensions for Rhai).
To play with the language itself, the [Rhai
playground](https://rhai.rs/book/start/playground.html) runs it in your
browser; the game's functions (`tell`, `players` and the rest) exist only
in the game and `bri-addon-run`.

## A rule is a set of functions

A script only defines functions; the game calls them when something
happens (the hooks in [Game rules](rules.md)). A line outside a function
is refused when the Add-On loads.

```rhai
fn on_join(player) {
    tell(player, "Welcome!");
}

// A helper of your own. The last expression is what it returns;
// `return` works too.
fn greeting(name) {
    `Hello, ${name}!`
}
```

A function sees only its own arguments and variables, not another
function's, and changing an argument inside it does not change the
caller's copy. What should last between calls goes in the rule's state
(`get`, `set`, `get_player`, `set_player`), declared in `behaviour.json`.

## Values

```rhai
let coins = 5;            // a whole number
let price = 2.5;          // a number with a fraction
const MAX = 10;           // never changes
let name = "Blockhead";   // text
let alive = true;         // true or false
let nothing = ();         // "no value", like null
coins += 1;
```

- `7 / 2` is `3` (whole numbers stay whole); `7.0 / 2.0` is `3.5`. Mixing
  them, as in `coins * price`, gives a fraction. `coins.to_float()` and
  `price.to_int()` convert.
- Text joins with `+`, numbers included: `"coins " + coins`. Backticks
  fill in values: `` `${name} has ${coins} coins` ``.
- `type_of(x)` names a value's type (`"i64"`, `"f64"`, `"string"`,
  `"array"`, `"map"`, `"()"`), handy when a call refuses an argument.

## Lists and maps

```rhai
let colors = ["red", "green", "blue"];
colors.push("white");
colors[0]           // "red": lists count from 0
colors.len()        // 4

let me = #{ name: "Blockhead", coins: 3 };
me.coins += 1;
me.name             // "Blockhead"
"coins" in me       // true
```

A map is written `#{ ... }`, with a `#`. Players, bricks and the other
things the game hands you read like maps: `player(p).name`, `p.alive`.

## Choices and loops

```rhai
if coins > MAX {
    tell(player, "Rich!");
} else if coins == 0 {
    tell(player, "Broke.");
} else {
    tell(player, "Getting there.");
}

for c in colors { print(c); }
for i in 0..3 { print(i); }       // 0, 1, 2
while coins > 0 { coins -= 1; }
loop { if done() { break; } }

let size = switch coins {
    0 => "none",
    1..5 => "few",
    _ => "many",
};
```

`continue` skips to the next turn of a loop, `break` leaves it. Braces
`{ }` are always needed around a block, even for one line.

## Text

`name.len()`, `name.to_upper()`, `name.contains("head")`,
`name.sub_string(0, 5)` and `"a b c".split(" ")` cover most needs. One
script value holds at most 4096 bytes of text.

## Seeing what happens

`print(...)` writes a line, after your Add-On's id, to the host's console
(the `~` key in a game you host), its log and `bri-addon-run`'s output;
players never see it. `tell(player, ...)` and `broadcast(...)` write to
chat. Try a rule without opening the game:

```sh
bri-addon-run path/to/my-rule --send go
```

In a game you host, save your edit and type `/reloadrules` to run the new
script at once, keeping the game's state
([Making Add-Ons](README.md), "Edit while you play").

## Mistakes and what they mean

`bri-addon-check` names the file and line of each problem. The usual ones:

| It says | Usually |
|---|---|
| a syntax error, "Expecting ';'" | a missing `;` at the end of a line, or a missing `}` |
| `script.top_level` | a line outside a function; move it into one |
| "Function not found" | a misspelt function, or the wrong number of arguments |
| "Variable not found" | a misspelt name, or a variable from another function |
| "Data type incorrect" | a number where text was wanted, or the reverse; check with `type_of` |
| `script.budget` | the call did too much work at once; spread it over ticks |

## Coming from TorqueScript

| TorqueScript | Rhai |
|---|---|
| `%coins = 5;` | `let coins = 5;` |
| `$MyMod::Rounds` | `get("rounds")`, a key declared in `behaviour.json` |
| `"a" @ "b"`, `"a" SPC "b"` | `"a" + "b"`, `"a" + " " + "b"` |
| `function serverCmdGo(%client)` | `fn cmd_go(player)`, with `go` listed in `commands` |
| `echo("hi");` | `print("hi");` |
| `%a $= "x"` | `a == "x"` |
| `for (%i = 0; %i < 3; %i++)` | `for i in 0..3` |
| `getWordCount(%s)`, `getWord(%s, 0)` | `s.split(" ").len()`, `s.split(" ")[0]` |

[torque-equivalents.md](torque-equivalents.md) maps the game calls.
