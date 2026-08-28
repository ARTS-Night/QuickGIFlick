# Performance notes

QuickGIFlick takes ownership of each ScreenDelta CPU frame with
`into_readback()`, avoiding an additional full-frame allocation and copy.
Unchanged samples extend GIF delay instead of allocating another image.

The current three-second recording limit bounds worst-case raw memory. Longer
recordings need a bounded encoder queue and an explicit drop policy before they
are exposed in the UI.
