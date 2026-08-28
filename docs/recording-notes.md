# Recording behaviour

QuickGIFlick samples ScreenDelta at the requested output rate (currently 15
FPS). A missing desktop update is not a dropped frame: it extends the previous
GIF frame's delay. This preserves recording duration on static screens and
avoids redundant GIF image data.

The first frame still waits for a real desktop update, which is the current
Desktop Duplication baseline limitation. Selection UI, cursor composition, and
long-recording buffering remain future work.
