# Files and configuration

A source file uses the `.clipasm` extension and begins with:

```clipasm
clipasm 1
```

Declarations come next. Executable statements start after the declarations.
Declarations cannot appear later in the file.

## Layout

The lexer ignores spaces, tabs, and indentation. Newlines separate statements and
configuration fields. There is no semicolon syntax.

A block containing one statement may fit on one line:

```clipasm
clip { image("title.png", 2s) } as title
```

Multiple statements require newlines:

```clipasm
clip {
    image("title.png", 2s)
    zoom_in(8%)
} as title
```

This is invalid because the two statements have no separator:

```clipasm
clip { image("title.png", 2s) zoom_in(8%) }
```

The parser accepts newlines inside parentheses around comma-separated arguments.
Comments begin with `#` and continue to the end of the line.

## Configuration and declarations

```clipasm
clipasm 1

config {
    video {
        width = 1920
        height = 1080
        fps = 30000/1001
        color = sdr_bt709
    }
    audio {
        sample_rate = 48000
    }
    output = "generated/final.mp4"
}

input source: Video
param title: File = "assets/title.png"
param duration: Duration = 2s
param amount: Number = 8%
param count: Integer
param range: TimeRange
param fit: Keyword(cover, contain, stretch) = contain
```

Graph input types are `Video` and `Audio`. Scalar parameter types are `Number`,
`Integer`, `File`, `Duration`, `TimeRange`, and a declared `Keyword(...)` set.
Another program or the CLI must supply parameters that have no defaults.

Only the root file may set project media configuration or an output path.
Omitted fields use `width = 1280`, `height = 720`, `fps = 30`,
`color = sdr_bt709`, and `sample_rate = 48000`. Project audio is stereo, and
publication is MP4 only. Color is one closed profile instead of independent
primaries, transfer, matrix, and range settings, so authored configuration
cannot create an incoherent signal description.

Frame rate is an exact positive rational. `fps = 30` means exactly 30 frames per
second. `fps = 30000/1001` is approximately 29.97 frames per second. This
explicit non-integer rate produces different frame counts for many durations.
It is not the default.
