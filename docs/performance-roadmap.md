# Performance roadmap

QuickGIFlick follows ScreenDelta's capture roadmap:

1. consume dirty-region/unchanged metadata to avoid GIF image work;
2. add a bounded encode queue before any longer recording option;
3. measure capture, encode, memory, and queue pressure at 10/15/20/30 FPS;
4. only then evaluate resize, palette, or GPU-processing changes.

The current fixed three-second recorder intentionally avoids a worker system:
it has a bounded worst case and no UI thread yet. A worker abstraction before
the UI or long recording feature would be speculative complexity.
