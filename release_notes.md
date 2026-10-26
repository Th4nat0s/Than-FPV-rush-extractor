# Unreleased

- Moved notifications into the collapsible bottom task panel (#1).
- Decode long-GOP poster frames from the beginning when AVI indexes cannot seek into H.264 streams (#2).
- Seek through long-GOP footage by decoding from the input start, and keep the seek slider interactive (#3).
- Add a persisted, opt-in Mute videos setting for single and bulk exports (#4).
- Show a dismissible Letz Kwad splash screen for 1.5 seconds, with a persisted opt-out (#5).
- Add an About window with the Letz Kwad logo, project URL, and developer credits (#7).
- Use calendar date pickers for recording imports and session edits (#6).
- Keep the Letz Kwad logo and credits visible in About and remove duplicate splash title text (#7).
- Reduce player startup/seek buffering with indexed decoding, retain sequential recovery, and enable FFmpeg's internal codec-header extraction and EV300D ADPCM decoder (#16).
- Restore the charcoal/orange palette, DVR/film metadata icons, and numbered orange pagination (#8, #10, #12).
