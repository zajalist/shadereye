# shadereye terminal-cast — regenerate with: vhs media/loop.tape
# Reproduces the compile -> see -> fix -> see loop from media/loop.gif.
# Requires charmbracelet/vhs (https://github.com/charmbracelet/vhs).

Output media/loop.gif

Set FontSize 18
Set Width 1100
Set Height 720
Set Theme "Dracula"
Set Padding 24
Set TypingSpeed 30ms

Type "user > the blue channel looks dead in examples/raymarch_broken.glsl, fix it"
Enter
Sleep 600ms

Type "claude > Let me render it and look."
Enter
Sleep 400ms
Type "  > render_shader(source=examples/raymarch_broken.glsl, 256x144, t=0)"
Enter
Sleep 500ms
Type "    [image] red<->green gradient -- blue channel flat at 0"
Enter
Sleep 900ms

Type "claude > Blue is zero everywhere. 'float t = 1 / 2;' is suspect -- checking the reference."
Enter
Sleep 500ms
Type "  > lookup_reference(\"integer division\")"
Enter
Sleep 500ms
Type "    -> integer-division -- In GLSL, 1/2 == 0 (integer division). Use 1.0/2.0 for float results."
Enter
Sleep 900ms

Type "claude > That is the bug. Editing the line."
Enter
Sleep 400ms
Type "  - float t = 1 / 2;"
Enter
Sleep 300ms
Type "  + float t = 1.0 / 2.0;"
Enter
Sleep 500ms
Type "  > render_shader(source=examples/raymarch_broken.glsl, 256x144, t=0)"
Enter
Sleep 500ms
Type "    [image] blue channel now reads 0.5 -- fixed"
Enter
Sleep 600ms

Type "* done -- blue channel restored; integer division replaced with float division."
Enter
Sleep 2s
