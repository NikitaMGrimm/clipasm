# 7. Reuse a scene style across source files

The edit applies the same kind of zoom in two places with different amounts.
You will extract that familiar operation behind a small callable interface, then
use it twice without changing the rendered result.

Continue in the same project and keep `learning.clipasm` from
[Change a named scene after assembly](06-timeline-edit.md).

## 1. Define the reusable operation

Create a `programs` directory, then create `programs/scene_motion.clipasm`:

```clipasm
clipasm 1

input video: Video
param by: Number = 4%

zoom_in($video, $by)
```

This source file defines one program. `input video` declares its Video input,
and `param by` declares a Number parameter with a default. Its final Video
returns to the caller.

## 2. Import the program

In `learning.clipasm`, add the import after `config` and before the clip
declarations:

```clipasm
import "programs/scene_motion.clipasm" as scene_motion
```

The path is relative to the file containing the import. `scene_motion` is the
local call name.

## 3. Use the operation twice

In the meadow clip, replace:

```clipasm
zoom_in(4%)
```

with:

```clipasm
scene_motion
```

The omitted `by` parameter uses the program's `4%` default. Inside the final
`during` body, replace `zoom_in(2%)` with:

```clipasm
scene_motion(2%)
```

This call overrides the default for the subtler evening movement. Each call
consumes the nearest Video from its current stack and passes it into a separate
invocation of `scene_motion`.

## 4. Validate the complete source package

```console,ignore
clipasm validate learning.clipasm
clipasm render learning.clipasm
```

Validation checks both source files and still reports 108 frames. Rendering
produces the same 4.5-second edit as before.

## 5. Change the shared style

In `programs/scene_motion.clipasm`, change the default:

```clipasm
param by: Number = 6%
```

Validate and render again. The meadow now uses the stronger `6%` default, while
the evening remains at its explicit `2%` override. One interface controls the
shared style without removing local choices.

## Complete checkpoint

Your finished `programs/scene_motion.clipasm` should be:

```clipasm
{{#include ../../examples/programs/scene_motion.clipasm}}
```

Your finished `learning.clipasm` should be:

```clipasm
{{#include ../../examples/learning-journey.clipasm}}
```

You have now followed one project from a single image through stack
composition, named clips, transforms, a transition, placement-based editing,
and an imported source program.

Continue with the [How-to guides](../index.md#how-to-guides) when you have a
specific task, or use the [Language reference](../reference/language/index.md)
for exact rules.
