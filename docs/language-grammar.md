# ClipAsm language grammar

This page is the normative EBNF grammar for ClipAsm language version 1. The
[language reference](language-reference.md) defines semantic constraints that
cannot be expressed by context-free grammar, including declaration uniqueness,
program signatures, scalar types, stack behavior, and required arguments.

The notation uses `[...]` for an optional form, `{...}` for zero or more
repetitions, and `|` for alternatives. Literal source characters appear in
quotes.

## Lexical grammar

```ebnf
letter           = "A"…"Z" | "a"…"z" ;
digit            = "0"…"9" ;

identifier       = (letter | "_"),
                   { letter | digit | "_" | "-" } ;

number           = digit, { digit },
                   [ ".", digit, { digit } ] ;

string           = '"', { string-character | escape }, '"' ;
escape           = '\\"' | "\\\\" | "\\n" | "\\r" | "\\t" ;

newline          = "\n" ;
horizontal-space = " " | "\t" | "\r" ;
comment          = "#", { character - newline }, [ newline ] ;
```

Horizontal space and comments are ignored. Newlines remain tokens because they
separate declarations, statements, and configuration fields. Keywords such as
`config`, `param`, and `as` use the `identifier` lexical form and acquire their
meaning from their grammar position.

## File and declarations

```ebnf
source-file         = { newline },
                      version-declaration, statement-end,
                      { declaration, statement-end },
                      { statement, { newline } } ;

version-declaration = "clipasm", "1" ;

declaration         = config-declaration
                    | import-declaration
                    | external-declaration
                    | input-declaration
                    | parameter-declaration ;

config-declaration  = "config", "{", { newline },
                      { config-field, statement-end },
                      "}" ;

config-field        = video-config
                    | audio-config
                    | "output", "=", string ;

video-config        = "video", "{", { newline },
                      { video-field, statement-end },
                      "}" ;

video-field         = "width", "=", number
                    | "height", "=", number
                    | "fps", "=", number, [ "/", number ] ;

audio-config        = "audio", "{", { newline },
                      { "sample_rate", "=", number, statement-end },
                      "}" ;

import-declaration  = "import", string, "as", identifier ;

external-declaration = "external", "{", { newline },
                       { external-field, statement-end },
                       "}" ;

external-field      = "executable", "=", string
                    | "arguments", "=", external-arguments
                    | "semantic_version", "=", number
                    | "preserve", "=", identifier ;

external-arguments  = "[", { newline },
                      [ external-argument,
                        { ",", { newline }, external-argument },
                        [ "," ] ],
                      "]" ;

external-argument   = string | "file", "(", string, ")" ;

input-declaration   = "input", identifier, ":", value-type ;

parameter-declaration = "param", identifier, ":", parameter-type,
                        [ "=", scalar-expression ] ;

value-type          = "Video" | "Audio" ;

parameter-type      = "Number"
                    | "Integer"
                    | "File"
                    | "Duration"
                    | "TimeRange"
                    | "Keyword", "(", identifier,
                      { ",", identifier }, ")" ;
```

All declarations precede the first statement. A `statement-end` is one or more
newlines, the closing brace of the containing block, or end of file.

## Statements and invocations

```ebnf
statement           = ( scalar-binding
                      | statement-expression, [ output-binding ] ),
                      statement-end ;

scalar-binding      = identifier, "=", scalar-expression ;

statement-expression = invocation
                     | reference-expression
                     | stack-block ;

invocation          = [ access ], identifier, [ type-argument ],
                      [ arguments ], [ block ] ;

access              = "@owned" | "@visible" ;
type-argument       = "<", value-type, ">" ;

arguments           = "(", { newline },
                      [ argument,
                        { ",", { newline }, argument },
                        [ "," ] ],
                      ")" ;

argument            = [ identifier, "=" ], argument-expression ;

argument-expression = invocation
                    | stack-block
                    | scalar-expression ;

block               = "{", { newline }, { statement, { newline } }, "}" ;
stack-block         = [ access ], block ;

reference-expression = "$", identifier ;

output-binding      = "as", identifier
                    | "as", "(", identifier, ",", identifier,
                      { ",", identifier }, ")" ;
```

Whether an identifier-led argument expression is an invocation is determined
syntactically by a following `(`, `<`, or `{`. Program lookup and the
classification of graph versus scalar arguments happen after parsing.

At statement position, absent and empty `arguments` are semantically
equivalent. After program resolution, an absent `block` becomes an empty body
for a body program and remains absent for a program that does not accept a
caller body. Sugar applies the same rule when it defines a body-capable
construct. Consequently `join`, `join()`, `join {}`, and `join() {}` are
equivalent before normal binding and body-contract validation.

A scalar binding is immutable, has no stack effect, and infers its scalar type
from the right-hand expression. Scalar bindings, parameters, inputs, and graph
output names share one program-wide namespace. Scalar bindings may refer
forward to other scalar bindings; cycles are errors. A timeline selector inside
a scalar binding resolves only from its explicit root and never borrows a later
invocation's contextual timeline root.

## Scalar expressions

```ebnf
scalar-expression  = range-expression ;

range-expression   = sum-expression,
                     [ "..", sum-expression ] ;

sum-expression     = product-expression,
                     { ("+" | "-"), product-expression } ;

product-expression = unary-expression,
                     { ("*" | "/"), unary-expression } ;

unary-expression   = { "+" | "-" }, postfix-expression ;

postfix-expression = primary-expression,
                     { "%" | "ms" | "s" } ;

primary-expression = number
                   | string
                   | identifier
                   | reference-expression
                   | timeline-selector
                   | "(", scalar-expression, ")" ;

timeline-selector  = "$", identifier,
                     "::", identifier,
                     { "::", identifier } ;
```

Postfix operators associate from left to right and may repeat. Thus `800%%`
means `(800 / 100) / 100`. The grammar deliberately does not reject unusual
compositions; checked scalar types determine whether each operation exists.

`%` requires Number and divides it by 100. `ms` and `s` require an expression
whose exact result satisfies Integer and construct Duration. Number supports
`+`, `-`, `*`, and `/`; Duration supports unary signs and
`Duration + Duration` or `Duration - Duration`. `..` requires two Duration
expressions and constructs TimeRange. A timeline selector ending in `start`,
`middle`, or `end` denotes a coordinate; a selector ending in a placement name
denotes that placement's complete closed-open range. Two coordinates with the
same timeline root may also be joined with `..` to construct a frame-native
TimeRange. Timeline coordinates with the same root support `+` and `-`; Number
may scale a coordinate with `*`, and a coordinate may be divided by Number.
Duration may offset a coordinate with `+` or `-`. These expressions are checked
for exact frame alignment and bounds only when consumed as a TimeRange.
