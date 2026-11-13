# Changelog

## 2.0.2 — 2026-11-14

- New reminder button in the title bar when a newer version is out: it stays after the new-version card is closed or hidden with Don't show again, and a click reopens the card with the release notes and the download link
- New News button in the title bar while yaiol has an announcement for the app: a click reopens the announcements, including the ones hidden with Don't show again. It does not appear when Show news from yaiol is off
- Don't show again can be undone: reopen a card from its title-bar button and untick the box, and it shows again at the next launch
- A video file given to the app when it starts (another program opening the file with it, or a path on the command line) is added to the work view
- The About page and the package carry a new one-line description of the app, and the Settings button's tooltip reads "Settings" instead of "Open settings"
- The sliders of the effects and the volume slider under the picture have a new look

## 2.0.1 — 2026-11-13

- The app draws its own title bar: the app icon, then Settings, Help and GitHub at its start, the name and version in the middle, and minimize, maximize and close at its end; the header under it keeps only the view and file buttons
- On Windows the title bar follows the system's title-bar colour, and a right-click on it (or a click on its icon) opens the system's window menu
- The ffmpeg manifest is read from https://apps.yaiol.com/data/ffmpeg.json
- README rewritten, with a logo and a screenshot

## 2.0.0 — 2026-11-06

- The app is renamed FFmpeg Video Studio (it was Video Converter); the installer, the Start menu entry and the settings folder carry the new name
- The work view now has the sound to work on as well: Video Effects and Audio Effects are two lists, each with standard effects as rows with a switch and the rest added from a menu; a sound band under the film strip carries the fades
- Fades are set by dragging a handle at the top of the film strip or the sound band, or by typing the seconds
- Drawers for the sound's channels, equaliser, compressor and level replace long forms; the equaliser and the compressor are drawn as graphs, and the panel under the picture can be hidden
- Channels, level change and loudness are effects of the Audio Effects list instead of preset settings; presets that had a channel choice are converted automatically
- The Encoder setting: a preset can use the default encoder or software, and Settings chooses the default (automatic or software); a button checks the encoders again, and the hardware-specific hint texts and the Quality (QP) setting are gone
- Settings > ffmpeg lets you choose which effects the work view always lists
- A panel shows the exact ffmpeg command for the selected file, with a Copy button
- The batch list shows a small picture taken from each video, and its empty list says to drop video files for batch processing
- Number fields can be dragged with the mouse, and the work view's controls are compact

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
