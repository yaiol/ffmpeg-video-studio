<p align="center">
  <img src="docs/assets/logo.png" alt="FFmpeg Video Studio" width="110" height="110">
</p>

<h1 align="center">FFmpeg Video Studio</h1>

<div align="center">
  <strong>Cut, grade, fade and convert video, with ffmpeg as the engine.</strong><br>
  See and hear what the conversion will make, then convert one file or a whole folder - without typing a command.
</div>

<br>

<!-- readme:nav -->

<div align="center">
  <a href="../../releases/latest"><img src="https://img.shields.io/github/v/release/yaiol/ffmpeg-video-studio?color=5a4fff&label=release&style=flat-square" alt="Release"></a>
  <a href="../../releases"><img src="https://img.shields.io/github/downloads/yaiol/ffmpeg-video-studio/total?color=5a4fff&label=downloads&style=flat-square" alt="Downloads"></a>
</div>

<h3 align="center">
  <a href="https://apps.yaiol.com/p/ffmpeg-video-studio/">Website</a>
  <span>&nbsp;·&nbsp;</span>
  <a href="#install">Install</a>
  <span>&nbsp;·&nbsp;</span>
  <a href="#effects">Effects</a>
  <span>&nbsp;·&nbsp;</span>
  <a href="#what-it-is">Features</a>
  <span>&nbsp;·&nbsp;</span>
  <a href="#documentation">Documentation</a>
  <span>&nbsp;·&nbsp;</span>
  <a href="#build-from-source">Development</a>
</h3>

<div align="center">
  <sub><a href="https://apps.yaiol.com/p/ffmpeg-video-studio/help/"><b>Help in 28 languages</b></a></sub>
</div>

<!-- /readme:nav -->

---

<p align="center">
  <img src="docs/assets/hero.png" alt="FFmpeg Video Studio showing a video with its film strip, sound band and effect drawers" width="900">
</p>

---

## Install

<div align="center">

<table align="center">
<tr>
<th align="center">Windows</th>
<th align="center">macOS</th>
<th align="center">Linux</th>
</tr>
<tr>
<td align="center"><a href="../../releases/latest"><img src="https://img.shields.io/badge/Windows-.exe-5a4fff?style=for-the-badge&amp;logo=windows&amp;logoColor=white" alt="Windows"></a></td>
<td align="center"><a href="../../releases/latest"><img src="https://img.shields.io/badge/macOS-.dmg-5a4fff?style=for-the-badge&amp;logo=apple&amp;logoColor=white" alt="macOS"></a></td>
<td align="center"><a href="../../releases/latest"><img src="https://img.shields.io/badge/Linux-.deb%20%7C%20.AppImage-5a4fff?style=for-the-badge&amp;logo=linux&amp;logoColor=white" alt="Linux"></a></td>
</tr>
<tr>
<td align="center">x64 installer</td>
<td align="center">Apple Silicon</td>
<td align="center">deb package or AppImage</td>
</tr>
</table>

</div>

> **Windows note:** SmartScreen may warn on first launch because the app is not code-signed. Click "More info", then "Run anyway".
>
> Windows is the platform the app is developed and tested on; the macOS and Linux packages are built from the same source on release and have had less use.

---

## What it is

FFmpeg Video Studio is a graphical studio on top of ffmpeg, with two views, chosen by the first two buttons of the header:

- **The work view (the studio)** opens first. One video at a time: a picture you can play with its sound, a film strip and a sound band for a timeline, a cut set with two buttons, fades you drag, and drawers for colour, curves, crop and rotate, texts, watermarks and the sound. You see and hear what the conversion will make before you make it.
- **The batch view** converts many files at once: add files or whole folders and convert them, each with its own settings.

In both views the choices are kept separate: the **preset** (how the video is encoded), the **size**, the **audio**, the **tags**, the **effects** and the **trim** are each chosen on their own, so one preset serves every combination. ffmpeg is not hidden: a panel shows the exact command that will run, and a button copies it.

---

## Effects

Effects are what is done to the picture and to the sound, and they are the heart of the studio. There are two lists, **Video Effects** and **Audio Effects**, and they work in both views.

- **Standard effects as switches.** The work view lists the effects most people need as rows you switch on and off while the video plays. For the picture: crop and rotate as one frame you drag over the image, colour grading with wheels or bars, curves, texts and watermarks as boxes you place on the picture, and the picture fade. For the sound: trim silence, keep a range, channels, equaliser, compressor, the four ways to set the volume, fades, silence at the end and ReplayGain tags. Several have a drawer of their own under the picture.
- **Every ffmpeg filter, on top.** The **Add an effect** menu offers all of ffmpeg's video and audio filters, grouped by category and searchable by name, each with ordinary controls generated from the filter itself: times such as 1:30, a colour picker, a chooser for files and folders, and a button that puts a value back to its default. Which effects the work view lists ready to switch on is your choice in Settings.
- **You set the order.** Effects run from the top of the list to the bottom as one ffmpeg filter chain; drag a card to change the result. A line warns when an effect counts on something a later one changes, and the app never reorders your list.
- **The sound follows the picture.** A change of tempo or a cut in the sound is followed by the picture so the two stay the same length.
- **Saved lists for the batch.** In the batch view an effects entry is a named list kept in Settings and chosen from a drop-down, so one entry goes with any preset.

The whole list, from the catalogue that comes with ffmpeg:

<!-- generated:effects:begin -->

<details>
<summary><b>All the Video Effects</b></summary>

<p><strong>Colour</strong> - Brightness, tint, white balance, curves</p>
<ul><li><strong>1D lookup table</strong>: Apply a 1D colour table from a .cube file.</li><li><strong>Auto contrast</strong>: Stretch the picture so its darkest and lightest points use the full range.</li><li><strong>Auto white balance</strong>: Remove an overall colour cast by assuming the scene averages to grey.</li><li><strong>Black and white</strong>: Turn the picture grey, choosing which colours come out lighter.</li><li><strong>Brightness, contrast, saturation, gamma</strong>: The four classic sliders in one filter.</li><li><strong>Colour</strong>: Primary colour correction as in a grading panel: lift, gamma, gain and offset with a master and a red, green and blue bar each, plus temperature, tint, contrast around a pivot, saturation, hue and colour boost. Everything at its neutral value does nothing.</li><li><strong>Colour balance</strong>: Push the shadows, mid-tones and highlights separately towards red, green or blue.</li><li><strong>Colour contrast</strong>: Raise the contrast between the red, green and blue parts of the picture.</li><li><strong>Colour curves</strong>: Reshape the tones with a curve for the whole picture and one for each of red, green and blue, as in a grading panel: drag a point of the line up or down to lighten or darken that part of the picture. A straight line from the bottom left to the top right does nothing.</li><li><strong>Colour LUT</strong>: Apply a colour look stored in a .cube file.</li><li><strong>Curves</strong>: Reshape the tones with a curve, as in a photo editor.</li><li><strong>Equalise the histogram</strong>: Spread the tones evenly: a strong automatic contrast.</li><li><strong>Exposure</strong>: Make the whole picture lighter or darker, like a camera's exposure.</li><li><strong>Hue and saturation</strong>: Rotate the colours around the colour wheel, or make them stronger or weaker.</li><li><strong>Hue, saturation and intensity by colour range</strong>: Like Hue and saturation, but for chosen colour ranges.</li><li><strong>Levels</strong>: Set the black point and white point of each colour, as an image editor does.</li><li><strong>Limit pixel values</strong>: Clamp every colour value into a range you choose.</li><li><strong>Lookup table (RGB)</strong>: Compute a colour table from a formula, on red, green and blue.</li><li><strong>Lookup table (YUV)</strong>: Compute a colour table from a formula, on brightness and colour planes.</li><li><strong>Lookup table by expression</strong>: Compute a colour table from a formula.</li><li><strong>Mix the colour channels</strong>: Mix red, green and blue into each other: sepia, swapped colours, tinted greys.</li><li><strong>Selective colour</strong>: Change one range of colours (reds, blues...) without touching the others.</li><li><strong>Shuffle colour planes</strong>: Swap the planes the picture is made of: strange, deliberate colours.</li><li><strong>Swap U and V</strong>: Swap the two colour components: skin turns blue.</li><li><strong>Table from two frames</strong>: Compute a colour table from two following frames.</li><li><strong>Temporal equalisation</strong>: Even out the tones over several frames.</li><li><strong>Tone mapping (HDR to normal)</strong>: Convert high-dynamic-range video to standard range. Needs a correct HDR chain.</li><li><strong>Vibrance</strong>: Boost the dull colours more than the strong ones: a gentler saturation.</li><li><strong>Warmth</strong>: Make the picture warmer (orange) or cooler (blue).</li><li><strong>White balance for shadows and highlights</strong>: Fix a colour cast in the darkest and lightest parts separately.</li></ul>
<p><strong>Sharpen and denoise</strong> - Crisper pictures, less noise and grain</p>
<ul><li><strong>Denoise (2D DCT)</strong>: A frequency-based denoiser. Slow.</li><li><strong>Denoise (3D FFT)</strong>: A frequency-based denoiser working over time. Slow.</li><li><strong>Denoise (non-local means)</strong>: A careful denoiser. Slow.</li><li><strong>Denoise (overcomplete wavelets)</strong>: A wavelet denoiser. Very slow.</li><li><strong>Denoise (wavelets)</strong>: A wavelet denoiser with several settings.</li><li><strong>Median filter</strong>: Remove isolated specks and keep the edges.</li><li><strong>Median over time</strong>: Take the middle value of several frames to remove passing specks.</li><li><strong>Reduce colour noise</strong>: Clean the coloured speckles and leave the detail.</li><li><strong>Reduce noise</strong>: A fast, good denoiser for ordinary noisy video.</li><li><strong>Reduce noise over time</strong>: Average several frames to calm the grain.</li><li><strong>Remove grain</strong>: Take out film grain and small specks.</li><li><strong>Sharpen</strong>: Make the picture crisper (or softer) with an unsharp mask.</li><li><strong>Sharpen, adaptive</strong>: A sharpening that avoids making edges harsh.</li><li><strong>Strong denoise (very slow)</strong>: The most thorough noise remover of the list: block matching in three dimensions. Takes minutes per minute of video.</li></ul>
<p><strong>Blur and soften</strong> - Soft focus, smoothing, motion streaks</p>
<ul><li><strong>Average blur</strong>: A quick blur by averaging neighbouring pixels.</li><li><strong>Blur, keep outlines</strong>: Soften the picture without blurring the edges.</li><li><strong>Box blur</strong>: A simple, quick blur.</li><li><strong>Directional blur</strong>: Blur in one direction: a motion streak.</li><li><strong>Edge-preserving blur</strong>: Another blur that keeps the outlines sharp.</li><li><strong>Gaussian blur</strong>: A smooth blur of a strength you choose.</li><li><strong>Shape-adaptive blur</strong>: A blur that follows the shapes in the picture.</li><li><strong>Smooth but keep the edges</strong>: Soften the picture while keeping the outlines sharp, as a skin-smoothing or cartoon look does.</li><li><strong>Smooth, keep edges (bilateral)</strong>: A smoothing that respects edges: skin softening, flat look.</li></ul>
<p><strong>Shape and position</strong> - Borders, rotation, perspective, zoom, upscaling</p>
<ul><li><strong>360 video projection</strong>: Convert 360-degree video between projections, or look into it.</li><li><strong>Add borders</strong>: Surround the picture with a coloured border or bars.</li><li><strong>Correct the perspective</strong>: Straighten a picture filmed at an angle by moving its four corners.</li><li><strong>Crop</strong>: Cut a strip off any edge of the picture.</li><li><strong>Crop and rotate</strong>: Turn the picture by any angle (a quarter or half turn is made exactly, with no empty corners) and cut a strip off each edge: one frame set over the picture. The turn comes first, so the corners it leaves empty are black unless the frame cuts them off.</li><li><strong>Cut a picture into frames</strong>: Split one tiled picture into a sequence of pictures.</li><li><strong>Fill the borders</strong>: Fill the edges of the picture, for example after stabilising.</li><li><strong>Fit on a blurred background</strong>: Fit the picture inside a frame of another shape, and fill the rest with a blurred, enlarged copy of itself.</li><li><strong>Flip</strong>: Mirror the picture.</li><li><strong>Resize</strong>: Make the picture bigger or smaller.</li><li><strong>Resize with colour-space control</strong>: A resizer that also converts the colour space and bit depth.</li><li><strong>Rotate</strong>: Rotate the picture a quarter or a half turn, for a video shot with the phone on its side.</li><li><strong>Rotate by any angle</strong>: Turn the picture by any angle, not only a quarter turn.</li><li><strong>Scroll the picture</strong>: Move the picture past at a steady speed, like a ticker.</li><li><strong>Slant</strong>: Shear the picture sideways or up and down.</li><li><strong>Stereoscopic 3D</strong>: Convert between the ways a 3D picture is stored: side by side, over and under, one eye at a time.</li><li><strong>Upscale pixel art (EPX)</strong>: Enlarge pixel art by 2 with the EPX method.</li><li><strong>Upscale pixel art (hqx)</strong>: Enlarge pixel art by 2, 3 or 4 with the hqx method.</li><li><strong>Upscale pixel art (Super2xSaI)</strong>: Enlarge pixel art by 2.</li><li><strong>Upscale pixel art (xBR)</strong>: Enlarge pixel art by 2, 3 or 4 with the xBR method.</li><li><strong>Zoom and pan</strong>: Slowly zoom into or across the picture. Made for stills: on video it needs care.</li></ul>
<p><strong>Time and frames</strong> - Frame tricks: time-lapse, smooth motion, padding</p>
<ul><li><strong>Change the frame rate, blended</strong>: Raise or lower the frame rate by blending neighbouring frames.</li><li><strong>Drop duplicate frames</strong>: Remove frames that repeat the one before, for video that was stretched from a lower frame rate.</li><li><strong>Drop near-duplicate frames</strong>: Remove frames that repeat the one before.</li><li><strong>Fade</strong>: Fade the picture in from black at the start of the part that is kept, and out to black at its end. Each length is in seconds; 0 leaves that end as it is.</li><li><strong>Frame rate</strong>: Force a constant number of pictures per second.</li><li><strong>Keep one frame in N</strong>: Keep every Nth frame: a quick time-lapse look.</li><li><strong>Pad the start or end</strong>: Hold the first or last frame, or add black, before or after. Changes the length.</li><li><strong>Pick the best frame of each group</strong>: Keep one representative frame out of every group of frames.</li><li><strong>Play backwards</strong>: Run the clip from its end to its start, sound included. The whole clip is held in memory.</li><li><strong>Random frames</strong>: Show frames in a random order. A gimmick.</li><li><strong>Repeat frames</strong>: Repeat a stretch of frames a number of times. Changes the length.</li><li><strong>Shuffle the frames</strong>: Put the frames in an order you give.</li><li><strong>Smooth motion</strong>: Invent the frames between frames, for smooth slow motion. Slow.</li><li><strong>Speed (change timestamps)</strong>: Play faster or slower by changing when each frame is shown. Changes the length.</li><li><strong>Trim</strong>: Keep one section of the clip and drop the rest.</li></ul>
<p><strong>Fix problems</strong> - Banding, flicker, shake, interlacing, logos, lens</p>
<ul><li><strong>Apply telecine</strong>: Add a telecine pattern (film to video).</li><li><strong>Correct lens distortion</strong>: Straighten the bulging of a wide-angle lens.</li><li><strong>Deinterlace</strong>: Remove the comb pattern that interlaced video shows on moving things.</li><li><strong>Deinterlace (bwdif)</strong>: A good deinterlacer, an alternative to the built-in one.</li><li><strong>Deinterlace (edge tracing)</strong>: A deinterlacer that follows edges.</li><li><strong>Deinterlace (kernel)</strong>: A simple kernel-based deinterlacer.</li><li><strong>Deinterlace (motion compensated)</strong>: A deinterlacer that follows motion. Slow.</li><li><strong>Deinterlace (neural network)</strong>: A high-quality deinterlacer. Slow.</li><li><strong>Deinterlace (Weston three-field)</strong>: A deinterlacer using three fields.</li><li><strong>Extract a field</strong>: Keep only the top or bottom field.</li><li><strong>Field matching from hints</strong>: Match fields using a hints file.</li><li><strong>Interlace over time</strong>: Combine frames into interlaced fields over time.</li><li><strong>Interleave or deinterleave fields</strong>: Rearrange the lines of the fields.</li><li><strong>Make interlaced</strong>: Turn progressive video into interlaced.</li><li><strong>Pullup (undo telecine)</strong>: Rebuild whole frames from a telecined field sequence.</li><li><strong>Reduce dot crawl</strong>: Clean the dots and rainbows of old analogue video.</li><li><strong>Remove a logo</strong>: Blend away a logo inside a rectangle you give.</li><li><strong>Remove banding, quick</strong>: A fast way to smooth gradients.</li><li><strong>Remove colour banding</strong>: Smooth the visible steps in skies and gradients.</li><li><strong>Remove compression blocks</strong>: Soften the squares an over-compressed video shows.</li><li><strong>Remove flicker</strong>: Even out brightness that pumps up and down between frames.</li><li><strong>Remove judder</strong>: Remove the stutter that pulldown leaves.</li><li><strong>Repeat fields</strong>: Apply the repeat-field flags of MPEG video.</li><li><strong>Set the field order</strong>: Swap which field comes first.</li><li><strong>Shift the field phase</strong>: Correct fields that are out of step.</li><li><strong>Smooth compression artefacts (fast)</strong>: A post-processing filter against block and ringing artefacts.</li><li><strong>Smooth compression artefacts (pp7)</strong>: Another post-processing filter against compression artefacts.</li><li><strong>Smooth compression artefacts (simple)</strong>: A simple post-processing filter against compression artefacts.</li><li><strong>Smooth compression artefacts (slow)</strong>: A slow, thorough post-processing filter.</li><li><strong>Soften flashes</strong>: Reduce flashes that can trigger photosensitive epilepsy.</li><li><strong>Split into fields</strong>: Split each frame into its two fields.</li><li><strong>Steady shaky video</strong>: A quick, one-pass stabiliser.</li><li><strong>Undo telecine (field matching)</strong>: Rebuild the original film frames of a movie that was converted to 29.97 interlaced video.</li><li><strong>Undo telecine (fixed pattern)</strong>: Reverse a telecine pattern from film-to-video transfers.</li><li><strong>Weave fields</strong>: Put pairs of fields back together into frames.</li><li><strong>Weave fields, double</strong>: Weave fields into twice as many frames.</li></ul>
<p><strong>Looks and styles</strong> - Creative effects: negative, pixelate, glitch, trails</p>
<ul><li><strong>Amplify motion</strong>: Exaggerate what changes between frames.</li><li><strong>Blend following frames</strong>: Combine each frame with the one before in a chosen way.</li><li><strong>Colour channel offset</strong>: Shift red, green and blue apart: a glitch or 3D-glasses look.</li><li><strong>Contact sheet</strong>: Tile several successive frames into one picture.</li><li><strong>Custom matrix filter</strong>: Apply a matrix of numbers you give (blur, emboss, sharpen).</li><li><strong>Dilate</strong>: Grow bright shapes with a structuring element.</li><li><strong>Edges (Kirsch)</strong>: An edge-detecting effect with a Kirsch operator.</li><li><strong>Edges (Prewitt)</strong>: An edge-detecting effect with a Prewitt operator.</li><li><strong>Edges (Roberts)</strong>: An edge-detecting effect with a Roberts cross operator.</li><li><strong>Edges (Scharr)</strong>: An edge-detecting effect with a Scharr operator.</li><li><strong>Edges (Sobel)</strong>: An edge-detecting effect with a Sobel operator.</li><li><strong>Equation per pixel</strong>: Compute every pixel from a formula.</li><li><strong>Erode</strong>: Shrink bright shapes with a structuring element.</li><li><strong>False colour</strong>: Paint brightness levels in colours, like a thermal camera.</li><li><strong>Film grain</strong>: Add noise or grain on top of the picture.</li><li><strong>Flood fill</strong>: Fill a region of one colour with another.</li><li><strong>Frequency-domain formula</strong>: Edit the picture's frequencies with expressions.</li><li><strong>Grow bright areas</strong>: Make bright areas larger (a morphological effect).</li><li><strong>Keep one colour, grey the rest</strong>: Everything turns grey except the colour you pick.</li><li><strong>Keep one colour, grey the rest (HSV)</strong>: Like the other colour-hold, picking the colour by hue.</li><li><strong>Keep one colour, grey the rest (YUV)</strong>: Like the other colour-hold, picking the colour differently.</li><li><strong>Make a mask</strong>: Turn pixels into a black-and-white mask by thresholds.</li><li><strong>Motion blur and ghosting</strong>: Mix several frames: smooth blur or echoes.</li><li><strong>Motion trails</strong>: Leave ghost trails behind moving things.</li><li><strong>Negative</strong>: Invert every colour, like a photographic negative.</li><li><strong>Outline</strong>: Draw only the edges: a sketch look.</li><li><strong>Pixelate</strong>: Turn the picture into big blocks.</li><li><strong>Posterise</strong>: Reduce the picture to a few flat colours.</li><li><strong>Scramble the pixels</strong>: Shuffle the pixels in blocks.</li><li><strong>Shift the colour layer</strong>: Move the colour layer off the picture.</li><li><strong>Shrink bright areas</strong>: Make bright areas smaller (a morphological effect).</li><li><strong>Swap two rectangles</strong>: Exchange two areas of the picture.</li><li><strong>Tilt-and-shift</strong>: A strange time-sliced look where time varies across the picture.</li><li><strong>Tint with a colour</strong>: Give the whole picture a colour cast of your choosing.</li><li><strong>Vignette</strong>: Darken the corners, as old lenses and films do.</li></ul>
<p><strong>Draw on the picture</strong> - Boxes, grids, vector drawings</p>
<ul><li><strong>Burn in subtitles (styled ASS)</strong>: The same for styled ASS subtitles, with their own fonts and colours.</li><li><strong>Burn in the subtitles</strong>: Draw the first subtitle track of the file into the picture. The output then has no subtitle track. Works with text subtitles only.</li><li><strong>Draw a box</strong>: Draw a coloured rectangle, filled or only an outline.</li><li><strong>Draw a grid</strong>: Draw a grid of lines over the picture.</li><li><strong>Draw vector graphics</strong>: Draw shapes and paths from a vector description.</li><li><strong>Text</strong>: Write text on the picture, as many as you like. Each one has a zone: the text is broken into lines to fit its width, and it can show the time played or left.</li><li><strong>Text, every option</strong>: Write a line of text on the picture: a title, a name, a countdown.</li><li><strong>Watermark</strong>: Put a picture, a logo for example, in a corner.</li></ul>

</details>

<details>
<summary><b>All the Audio Effects</b></summary>

<p><strong>Time and frames</strong> - Frame tricks: time-lapse, smooth motion, padding</p>
<ul><li><strong>Play backwards</strong>: Run the clip from its end to its start, sound included. The whole clip is held in memory.</li></ul>
<p><strong>Volume and dynamics</strong> - Louder, quieter, even out, compress, limit, gate, normalise</p>
<ul><li><strong>Audio contrast</strong>: Make the sound punchier by expanding or compressing its dynamics lightly.</li><li><strong>Compressor</strong>: Everything above a level is lowered by a ratio, so the loud and the quiet parts get closer.</li><li><strong>Compressor / expander</strong>: Compress or expand the dynamics with a transfer curve you set.</li><li><strong>Dynamic equaliser</strong>: An equaliser whose gain follows the level of the sound.</li><li><strong>Dynamic normaliser</strong>: Even out the level over time, quiet and loud parts alike.</li><li><strong>Dynamic smoothing</strong>: Smooth sudden jumps in the sound.</li><li><strong>Limiter</strong>: Stop the sound going above a level without hard clipping.</li><li><strong>Loudness normaliser</strong>: Bring the sound to a standard loudness (EBU R128).</li><li><strong>Multiband compressor</strong>: Compress each frequency band on its own.</li><li><strong>Noise gate</strong>: Silence the sound when it falls below a level.</li><li><strong>Psychoacoustic clipper</strong>: Clip loud peaks in a way the ear notices less.</li><li><strong>ReplayGain: write tags</strong>: Write the track gain and peak of the new file as tags; the sound is untouched.</li><li><strong>Shift the DC level</strong>: Move the whole waveform up or down.</li><li><strong>Soft clipper</strong>: Round off peaks gently instead of cutting them.</li><li><strong>Spectral dynamic range control</strong>: Control the level of each frequency band separately.</li><li><strong>Speech normaliser</strong>: Bring speech to an even level.</li><li><strong>Volume</strong>: Make the sound louder or quieter.</li><li><strong>Volume: apply ReplayGain</strong>: Apply the ReplayGain gain of the track (or of the album) to the sound itself, with a limiter.</li><li><strong>Volume: fixed gain</strong>: Make the sound louder or quieter by a fixed amount, with a limiter.</li><li><strong>Volume: normalise to a loudness</strong>: Bring the loudness of the part that is kept to a target, with a limiter so the gain cannot clip.</li><li><strong>Volume: normalise to a peak</strong>: Bring the highest sample to a target level.</li></ul>
<p><strong>Equaliser and filters</strong> - Bass, treble, keep or cut a range of frequencies</p>
<ul><li><strong>18-band equaliser</strong>: An equaliser with 18 fixed bands.</li><li><strong>All-pass filter</strong>: Change the phase of the sound without changing its loudness.</li><li><strong>Band-pass filter</strong>: Keep a band of frequencies and remove the rest.</li><li><strong>Band-reject filter</strong>: Remove one band of frequencies, for example a hum.</li><li><strong>Bass</strong>: Boost or cut the low frequencies.</li><li><strong>Biquad filter from coefficients</strong>: Apply a two-pole, two-zero filter you give as numbers.</li><li><strong>Boost the bass</strong>: Add subwoofer-range frequencies.</li><li><strong>Cut the sub-bass</strong>: Remove the lowest frequencies.</li><li><strong>Cut very high frequencies</strong>: Remove frequencies above the range of hearing.</li><li><strong>Emphasis (RIAA and others)</strong>: Apply or undo a recording emphasis curve such as RIAA.</li><li><strong>Equaliser (one band)</strong>: Boost or cut one band of frequencies.</li><li><strong>Equaliser (ten bands)</strong>: A gain for each octave band, from 31 Hz to 16 kHz.</li><li><strong>FIR equaliser</strong>: A precise equaliser with a curve you describe.</li><li><strong>Frequency shift</strong>: Move every frequency up or down by a fixed amount.</li><li><strong>Frequency-domain expression filter</strong>: Change the sound with formulas applied to its frequencies.</li><li><strong>High shelf</strong>: Boost or cut everything above a frequency.</li><li><strong>High-order band-pass</strong>: Keep one frequency band with a very steep filter.</li><li><strong>High-order band-stop</strong>: Remove one frequency band with a very steep filter.</li><li><strong>High-pass filter</strong>: Remove the frequencies below a limit: rumble, hum.</li><li><strong>IIR filter from coefficients</strong>: Apply an infinite impulse response filter you give as numbers.</li><li><strong>Low shelf</strong>: Boost or cut everything below a frequency.</li><li><strong>Low-pass filter</strong>: Remove the frequencies above a limit: hiss.</li><li><strong>Parametric equaliser</strong>: Many-band equaliser with its own curve per channel.</li><li><strong>Phase shift</strong>: Shift the phase of all frequencies.</li><li><strong>Sound derivative</strong>: Compute the rate of change of the sound, a signal-processing tool.</li><li><strong>Sound from an expression</strong>: Rewrite each sample of the sound with a formula.</li><li><strong>Sound integral</strong>: Compute the running sum of the sound, a signal-processing tool.</li><li><strong>Spectral tilt</strong>: Tilt the balance between low and high frequencies.</li><li><strong>Tilt shelf</strong>: A shelf filter that tilts the whole frequency balance.</li><li><strong>Treble</strong>: Boost or cut the high frequencies.</li></ul>
<p><strong>Noise and repair</strong> - Hiss, clicks, clipping, hum, sibilance</p>
<ul><li><strong>De-esser</strong>: Soften harsh s sounds in speech.</li><li><strong>Decode HDCD</strong>: Decode High Definition Compatible Digital discs.</li><li><strong>Denoise (FFT)</strong>: Reduce steady noise such as hiss using frequency analysis.</li><li><strong>Denoise (non-local means)</strong>: Reduce broadband noise with a patch-matching method.</li><li><strong>Denoise (wavelets)</strong>: Reduce noise using wavelets.</li><li><strong>Denoise speech (neural network)</strong>: Reduce noise in speech with a trained neural network.</li><li><strong>Remove clicks</strong>: Take out short clicks and pops, as from old records.</li><li><strong>Repair clipping</strong>: Restore peaks that were cut off by too high a level.</li></ul>
<p><strong>Time and pitch</strong> - Fade, speed, pitch, cut, loop, silence</p>
<ul><li><strong>Change the rate without resampling</strong>: Play the same samples at another rate: pitch and speed change together.</li><li><strong>Change the speed, keep the pitch</strong>: Make the sound faster or slower without changing its pitch.</li><li><strong>Cut the sound</strong>: Keep one section of the sound and drop the rest.</li><li><strong>Delay channels</strong>: Delay one or more channels by a number of milliseconds.</li><li><strong>Distance delay</strong>: Delay the sound as if it travelled a distance, to line up speakers.</li><li><strong>Fade</strong>: Fade the sound in from silence at the start of the part that is kept, and out to silence at its end. Each length is in seconds; 0 leaves that end as it is.</li><li><strong>Fade in or out</strong>: Fade the sound in or out over a chosen time.</li><li><strong>Keep a range</strong>: Keep only the part between two times, counted on what the effects above it left.</li><li><strong>Limit the duration</strong>: Keep at most this many seconds from the start.</li><li><strong>Loop the sound</strong>: Repeat a section of the sound.</li><li><strong>Pad with silence</strong>: Add silence at the end, for a fixed length or a number of samples.</li><li><strong>Remove silence</strong>: Cut out silent stretches.</li><li><strong>Resample</strong>: Change the sample rate of the sound.</li><li><strong>Silence at the end</strong>: Cut the song where its trailing silence begins and leave exactly this much silence after it, so an LP plays as separate tracks.</li><li><strong>Time-stretch and pitch-shift</strong>: Change the speed and the pitch independently, with high quality.</li><li><strong>Trim silence at the ends</strong>: Cut the silence before the sound starts and after it stops - never silence inside the song.</li></ul>
<p><strong>Stereo and channels</strong> - Widen, pan, mix channels, headphones, surround</p>
<ul><li><strong>Channels</strong>: Swap left and right, play one channel on both sides, mix both into one, or mix any layout down to stereo.</li><li><strong>Decorrelate channels</strong>: Make the channels less alike, which widens the sound.</li><li><strong>Dialogue enhancer</strong>: Make speech stand out in a film mix.</li><li><strong>Extra stereo</strong>: Increase the difference between left and right.</li><li><strong>Haas stereo enhancer</strong>: Widen the sound with a tiny delay between the sides.</li><li><strong>Headphone crossfeed</strong>: Mix a little of each side into the other to ease headphone listening.</li><li><strong>Headphone stereo widening</strong>: Make the stereo image wider, tuned for headphones.</li><li><strong>Mix channels</strong>: Remix channels with your own coefficients: stereo to mono, swap sides.</li><li><strong>Remap channels</strong>: Move channels to other positions.</li><li><strong>Stereo tools</strong>: Balance, mid and side levels, mute, phase and more in one filter.</li><li><strong>Upmix to surround</strong>: Spread a stereo sound over a surround layout.</li><li><strong>Widen the stereo</strong>: Make the stereo image wider with a delayed feedback.</li></ul>
<p><strong>Sound effects</strong> - Echo, chorus, phaser, tremolo, distortion</p>
<ul><li><strong>Bit crusher</strong>: Reduce the sound's bit depth for a lo-fi, crunchy effect.</li><li><strong>Chorus</strong>: Thicken the sound with slightly detuned copies.</li><li><strong>Crystalizer</strong>: Sharpen the sound by exaggerating its changes.</li><li><strong>Echo</strong>: Add one or more echoes to the sound.</li><li><strong>Exciter</strong>: Add brightness by enhancing the high frequencies.</li><li><strong>Flanger</strong>: Add a sweeping, jet-like effect.</li><li><strong>Phaser</strong>: Add a sweeping phasing effect.</li><li><strong>Pulsator</strong>: Make the level pulse, with the stereo sides taking turns.</li><li><strong>Tremolo</strong>: Make the level wobble.</li><li><strong>Vibrato</strong>: Make the pitch wobble.</li><li><strong>Virtual bass</strong>: Make small speakers seem to play deeper notes.</li></ul>

</details>

<!-- generated:effects:end -->

---

## Features

- **A work view and a batch view** - work on one video with its timeline, or convert a whole folder at once.
- **Hear and see the result** - playback is real ffmpeg with your effects applied, with the sound; the part outside the cut is dimmed on a film strip, and fades are two handles on the timeline.
- **Video Effects and Audio Effects** - standard effects as switches, every ffmpeg filter in a menu, in an order you choose (see above).
- **Presets** - MKV, MP4, WebM, MOV and AVI with H.264, H.265, AV1, VP9, ProRes, uncompressed RGB or a straight copy; a quality number or a target bitrate, a speed, 10-bit, tunes and extra x265 parameters.
- **A graphics card when it helps** - NVIDIA, AMD or Intel encoders, chosen as a setting. The app tests which ones work on your computer and falls back to the processor when a preset cannot use a card.
- **Tags and chapters** - keep everything except a list of names, or remove everything except a list, with wildcards; chapters have their own switch.
- **Per-file settings** - every file in the batch carries its own preset, size, audio, tags, effects, cut and destination.
- **Where the files go** - beside the source or into a chosen folder, with the folder structure kept, and skip, overwrite or keep-both when a file exists.
- **Your language** - the interface is available in 52 languages.

---

## Documentation

| | |
|---|---|
| **User manual** | [Read it online](https://apps.yaiol.com/p/ffmpeg-video-studio/help/) |
| **What's new** | [Release notes](https://apps.yaiol.com/p/ffmpeg-video-studio/help/releases/) |
| **Product page** | [apps.yaiol.com](https://apps.yaiol.com/p/ffmpeg-video-studio/) |

---

## Build from source

Requires the Rust toolchain (stable) and, for the installer, the Dioxus CLI (`cargo install dioxus-cli`). The app drives **ffmpeg**, which is not part of the repository and is not bundled: the app downloads the build yaiol has approved into a folder of its own the first time it starts, and never uses an ffmpeg found elsewhere on the machine.

```bash
cargo run -p ffmpeg-video-studio                                # run in development
cargo test                                                      # engine and app tests (the conversion tests need ffmpeg)
cd app && dx bundle --desktop --release --package-types nsis    # Windows installer → release/
```

---

## Architecture

A Cargo workspace of two crates: the **engine** (everything that is not the window - presets, effects, the job queue, the ffmpeg driver and the preview, with no UI dependency) and the **app** (the Dioxus desktop shell, which polls the engine every 200 ms; the engine never calls back into the window).

| Layer | Technology |
|-------|-----------|
| UI | Dioxus 0.7 on the system WebView |
| Video engine | ffmpeg, a separate executable the app downloads and installs itself |
| Preview sound | rodio |
| Interface languages | flat JSON bundles embedded at build time |
| Packaging | `dx bundle` (NSIS, deb, AppImage, dmg) |

<details>
<summary><b>How the effects run</b></summary>

The effects of a file run in the order of the list as one ffmpeg filter chain, or one filter graph when an effect needs a second input such as a watermark. The size limit sits where it must, and an effect listed before another that changes what it works on is reported, never reordered. The sound has its own chain; a tempo change or a cut of the sound is followed by the picture so the two stay the same length.

</details>

<details>
<summary><b>How the preview works</b></summary>

The picture is one ffmpeg run built from the same plan as a conversion, so what shows is what the effects will do; it is served to the window as a stream of pictures from a small server on the local machine. The sound is a second ffmpeg played through the default output.

</details>

---

## License

The app downloads [ffmpeg](https://ffmpeg.org) as a separate executable from its builders; it is not part of this repository or of the installer and keeps its own licence.

<div align="center">
  <sub>FFmpeg Video Studio is part of <a href="https://apps.yaiol.com">yaiol Applications</a>.</sub>
</div>
