# Changelog

## 1.5.0 — 2026-10-27

- Effects are now a catalogue of ffmpeg's own effects, grouped by category, with each effect's options shown as ordinary controls; they run in the order listed and can be reordered by dragging
- The fixed turn/flip, watermark and text choices, the separate colour LUT choice and the "Remove the metadata" effect are gone; turns, watermarks, text and LUTs are now ordinary effects from the catalogue
- New Tags choice: keep or delete all tags, with exceptions by name (* matches any run of characters), and optionally remove the chapters; it works on a copied video too
- ffmpeg is installed by the app itself at launch, in a folder shared by all yaiol apps; a machine's own ffmpeg is no longer used, and the effects catalogue is fetched together with ffmpeg
- Settings > General shows the installed and latest ffmpeg versions with an Update button, and a popup shows the install progress
- The destination menu has "Pick Destination…", which saves the folder you choose as a destination
- New Notifications setting: a system notice when a conversion finishes
- A job whose effect names a missing file now fails with that file's name

## 1.0.0 — 2026-10-23

- Initial release
