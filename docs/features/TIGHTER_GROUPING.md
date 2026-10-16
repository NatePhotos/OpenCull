# Conservative time grouping

Time-only bursts now require neighbour gaps of at most 300 ms and a total span
of at most three seconds. Intelligent repeated-pose grouping now has a
three-second neighbour and total-span limit, with face geometry checked against
both the preceding frame and the group's first frame. This replaces the former
20-second pose window and prevents adjacent matches chaining an event together.
The 13-second P1962550/P1962551 example is outside the new window.

Face inference caches remain valid: these are grouping rules, not new face
models. Groups and ranks are recomputed from cached analysis when a session
opens. Time alone still cannot prove visual similarity within a short burst;
the optional MobileCLIP feature adds an image-appearance check.
