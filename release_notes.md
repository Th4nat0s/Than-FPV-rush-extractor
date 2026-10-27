# Unreleased

- Moved notifications into the collapsible bottom task panel (#1).
- Decode long-GOP poster frames from the beginning when AVI indexes cannot seek into H.264 streams (#2).
- Seek through long-GOP footage by decoding from the input start, and keep the seek slider interactive (#3).
- Add a persisted, opt-in Mute videos setting for player playback and exports (#4, #14).
- Show a dismissible Letz Kwad splash screen with a persisted opt-out and animated zoom/fade exit (#5).
- Add an About window with the Letz Kwad logo, project URL, and developer credits (#7).
- Use calendar date pickers for recording imports and session edits (#6).
- Keep the Letz Kwad logo and credits visible in About and remove duplicate splash title text (#7).
- Restore the charcoal/orange palette, DVR/film metadata icons, and numbered orange pagination (#8, #10, #12).
- Place player tags and figures beside the video, with non-collapsible fullscreen and close controls (#11, #13).
- Add a speaker icon beside the player volume slider (#15).
- Update the README for the current native Rust/egui workflow and controls (#9).
- Reduce player startup/seek buffering with indexed decoding, retain sequential recovery, and enable FFmpeg's internal codec-header extraction and EV300D ADPCM decoder (#16).
- Enlarge the splash to 2.5 times its initial size while fading out and remove its Close button (#5).
- Include the LetzKwad logo, exact role credits and website in both the native macOS and in-app About views (#7).
- Overlay seek, playback and audio controls on the video (#21).
- Keep the player inside the visible viewport, including after shrinking the app window (#17).
