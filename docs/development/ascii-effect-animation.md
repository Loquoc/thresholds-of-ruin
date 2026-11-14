# ASCII transient effect playback

The ASCII window draws newly validated ordered effect outcomes as temporary map
glyphs. Rays grow directional `- | / \\` trails; projectiles move a single `*`.
The event contract does not identify the thrown item, so the client does not invent
an arrow or weapon identity. Cue colors use only the server-disclosed category.

This is an independent implementation inspired by NetHack 3.6.7's `tmp_at`
`DISP_BEAM`/`DISP_FLASH` distinction in
[display.c](https://github.com/NetHack/NetHack/blob/NetHack-3.6.7_Released/src/display.c):
a beam retains its trail, a missile replaces its previous glyph, and cleanup
restores the ordinary map. No NetHack source was copied.

Playback uses elapsed presentation time (normally 60 ms per disclosed point,
capped at 1.8 seconds per event), never simulation time or sleeps. Network pumping,
input, messages, and modal screens continue. A maximum of 64 simultaneous events
bounds the queue; oldest cosmetic animations are dropped on overflow. Long paths
may skip intermediate frames at display frame rate. These durations are client
styling, not server effect lifetimes.

Only positions explicitly listed in each disclosed segment are rendered. Segment
boundaries clear the old trail; missing anchors split rather than bridge a path.
Stable opaque anchor keys are resolved against the current visible cells, so an
observer moving or losing sight never paints an old coordinate or hidden memory.
Full map redraw restores underlying glyphs. Snapshot replacement clears playback;
retained observation events never replay it.

Scope: instantaneous ray, projectile, and target presentations. Area delivery and
status indicators remain narration-only. Persistent `public_effects` cloud/burst/
cone lifecycle overlays are not implemented here; an area is not misleadingly
animated as a traveling projectile. This is not a new Rogue mechanic.

Tests in `turns_and_map` cover native Canvas overlay/restoration, ordered outcome
consumption, picture non-replay, input responsiveness, snapshot reset, directions,
bounces, gaps, visibility/key remapping, frame-rate independence, queue limits,
maximum protocol path duration, and the explicit area limitation. The overlay
pixel witness was first run without the renderer block and failed as intended.
