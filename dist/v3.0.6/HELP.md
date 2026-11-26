# Organimizer Help

Organimizer 3.0.6 by PrimeCrew Software (formerly **PhotoSorter**).
It renames files with ordered rules and sorts them into date folders such as
`March 2024` (the **Organize** tab), and it organizes movies and TV episodes
into the folder layout Plex expects (the **Plex Media** tab).

The full reference is `README.txt` next to `Organimizer.exe`.

Contents: Quick start (Organize) | Quick start (Plex Media) | Plex tab |
Movies | Movie extras | TV shows | Common mistakes | Cheat sheet |
File types and protection | Folder blacklist | Favorites |
Getting a TMDB API key | Looking up titles on TMDB |
Portable or installed | Coming from PhotoSorter | Safety |
Troubleshooting | Keyboard shortcuts | About and license

## Quick start: Organize

1. **Choose a folder.** Click **Browse...** (or File > Open Folder..., Ctrl+O),
   drag a folder onto the window, or pick one from the **Favorites** menu.
   "Include subfolders" is off by default.
2. **Choose the file types.** Next to "File types" pick *Images & videos*
   (the default, same as PhotoSorter), *Images*, *Videos*, *Audio*,
   *Documents*, *Archives*, *All files* or *Custom list* (Tools > Options).
3. **Add rename rules (optional).** In step 2 click **Add rule**. Rules
   run top to bottom on the name without its extension.
4. **Choose the output.** In step 3 leave **Sort into date folders** on to
   move each file into a folder like `March 2024` next to it, or turn it off
   to only rename.
5. **Check the preview**, then click **Apply** and confirm. An undo log is
   written first; **Undo last run** puts everything back.

## Quick start: Plex Media

1. Click the **Plex Media** tab at the top.
2. **Source:** choose the folder with your movies or episodes (downloads,
   rips, an old library) and click **Scan**. Nothing is changed. The Source
   box starts empty each time; **Recent** next to it lists the last 4
   folders you used.
3. **Mode:** **Rename only** (the default) when your folders are already
   right (`Show (Year)\Season 02`, `Movie (Year)`): each file is renamed
   where it is. **Full sort** for a mixed folder: Organimizer builds the
   Plex folders and moves the files (see "Rename only or Full sort" in the
   Plex tab chapter).
   In Full sort, **Destination:** *Inside source folder* builds the Plex
   folders inside the scanned folder; *Library folders* moves movies into
   your Movies library folder and episodes into your TV shows folder.
4. **Check the proposals.** Every video gets a type (Movie, TV episode,
   Extra, Skip) and a new name or location, for example
   `Heat (1995)\Heat (1995).mkv` or
   `Breaking Bad\Season 01\Breaking Bad - s01e02.mkv`. The colored dot shows
   how sure Organimizer is: **green** = high, **amber** = medium, **red** =
   low, please check. "Only files that need a look" hides the sure ones.
5. **Correct anything** by clicking the file name: the panel on the right
   lets you change the type, title, year, edition, season, episode, episode
   title, extra type and whether an extra belongs to a movie or a show.
   "Use for all files of this title" copies title and year to a whole
   season. Untick a row to leave that file alone.
6. Subtitles (`.srt`, `.ass`, `.sub`/`.idx`, `.vtt`, ...) next to a video or in
   a `Subs` folder are renamed with their video, keeping language tags such
   as `.en`, `.forced` or `.sdh`. Sample clips are skipped. Extras
   (trailers, featurettes, deleted scenes, ...) go into the Plex extras
   folders (`Trailers`, `Featurettes`, ...).
7. Click **Apply** and confirm. Existing files are never overwritten (a
   taken name gets ` (1)`), and **Undo last run** moves everything back.
   Old, now empty folders are left in place (nothing is ever deleted).

**Why this suggestion?** The **Why** column (and "Why this suggestion" in
the right-hand panel) says in plain words how each proposal was worked out,
for example `"making of" in the name, next to Alien (1979).mkv -> Behind The
Scenes` or `in a "Season2" folder -> Season 2`. Organimizer looks at the file
name, the files next to it, the parent and grandparent folder names, disc
rip names (`title_t00`, `LABEL_t03`, `VIDEO_TS`, `BDMV`) and the runtime:
when several files have the same title, the longest one is the movie and the
shorter ones become extras (under 2.5 minutes: trailer, under 45 minutes:
featurette). Names that are "almost right" are fixed to the exact Plex
spelling: `Alien.1979.Directors.Cut.mkv`, `The Expanse S2 E5.mkv`,
`Expanse - 2x05 - Doors and Corners.mkv`, `alien making of.mkv`, a `Season2`
folder, a year without parentheses and so on.

Optional: turn on the **TMDB lookup** (see the next sections) and click
**Look up** in "What you have" to confirm titles and episode names against
The Movie Database. You always choose the match yourself.

See the **Plex tab** chapter below for the official Plex Media
Server naming rules (movies, extras folders, TV shows, and examples).
The TMDB key and lookup steps are in **Getting a TMDB API key** and
**Looking up titles on TMDB**.

## Plex tab

This chapter explains the naming conventions Plex Media Server expects,
and how Organimizer uses them. The rules here follow Plex's official
support articles (listed at the end of this chapter). Organimizer does
not invent extra folder names or suffixes.

Open this chapter from the Plex Media tab with **Help > Plex naming
conventions...**, the **Plex naming conventions (Help)** button on an
empty Plex tab, or **Help (F1)** and the Contents line "Plex tab".
(Windows opens HELP.md in the default app for .md files, often Notepad,
which starts at the top of the file.)

### What the Plex tab does

1. Click the **Plex Media** tab.
2. **Source:** choose the folder with your movies or episodes and click
   **Scan**. Nothing is changed on disk. (**Recent** lists your last 4
   folders.)
3. **Mode:** **Rename only** (default) or **Full sort** - see below.
   In Full sort, **Destination:** *Inside source folder* builds the Plex
   folders inside the scanned folder; *Library folders* moves movies into
   your Movies library folder and episodes into your TV shows folder.
   Plex itself works best when movies and TV shows are in **separate
   library folders**.
4. Check the preview: type (Movie, TV episode, Extra, Skip), the new
   location, and the **Why** column (plain-language reasons). The colored
   mark is confidence: green = high, amber = medium, red = low. Click a
   row to correct the type, title, year, edition, season, episode or
   extra type. Untick a row to leave that file alone.
5. Optional **Look up** (needs a TMDB key: see **Getting a TMDB API
   key**). If the filename confidence is not High, or TMDB finds
   nothing, a prompt asks you to type the name and an optional 4-digit
   year (see **Looking up titles on TMDB** / **If it can't find your
   show**). Choosing a match still only writes the preview.
6. Click **Apply** and confirm. Existing files are never overwritten (a
   taken name gets ` (1)`). **Undo last run** moves everything back.
   Old empty folders are left in place (nothing is ever deleted).

### Rename only or Full sort

Pick the mode at the top of the Plex tab (step 2). If you change it after
a scan, click **Rescan**.

**Rename only** (the default) - your folders are already in place, the
file names are not. Most libraries are like this.

- Nothing moves to another folder and no folder is created. Each file is
  renamed inside the folder it is in now.
- The show name, year and `{tmdb-ID}` come from the show folder, and the
  season from the `Season NN` folder the file is in:
  `Breaking Bad (2008)\Season 02\breaking.bad.s02e05.720p.mkv` becomes
  `Breaking Bad (2008)\Season 02\Breaking Bad (2008) - s02e05.mkv`.
- Movies take their title and year from their own movie folder:
  `Heat (1995)\Heat.1995.1080p.BluRay.mkv` becomes
  `Heat (1995)\Heat (1995).mkv` (edition and `- pt1` parts are kept).
- You can choose a Season folder, a show or movie folder, or your whole
  library folder: Organimizer walks into the folders below it. When you
  choose a Season folder itself, the show folder above it is still used.
- Extras in an extras folder (`Featurettes`, `Trailers`, ...) stay there
  with a clean name. Extras next to the movie keep Plex's inline suffix
  (`Teaser-trailer.mkv`). Subtitles stay in their folder and take their
  video's new name.
- **When a folder looks wrong**, Organimizer does not guess: the row is
  marked amber with a note, for example a `Season 1` folder that is not
  inside a show folder, a movie that is not in its own folder, a file name
  that names a different show than its folder, or no year. Click
  **Look up** for that title to type the right name and year.
- **Folder name fixes** are offered as separate rows above the file list,
  **unticked**: `Season 2` -> `Season 02`, `Specials` -> `Season 00`,
  adding the year or `{tmdb-ID}` to a show or movie folder, and the
  official extras folder names (`Featurette` -> `Featurettes`). Tick only
  the ones you want. They are renamed after the files, and Undo puts the
  old folder names back. A fix whose new name already exists is shown in
  amber and never applied. The folder you chose itself is never renamed
  (it holds the undo log).

**Full sort** - a mixed folder of downloads or rips that needs the whole
Plex layout. Organimizer builds `Show (Year)\Season NN\` and
`Movie (Year)\` (with the extras folders) and moves the files into them,
either inside the chosen folder (*Inside source folder*) or into your
Movies and TV library folders (*Library folders*).

Both modes use the same preview, Apply, Undo last run and
never-overwrite rules.

### Movies

Plex's recommended layout is one folder per movie, with the same title
and year on the folder and the file:

    Movies/
      Title (Year)/
        Title (Year).ext

Example:

    Movies/
      Avatar (2009)/
        Avatar (2009).mkv
        Avatar (2009).en.srt

Stand-alone files in the Movies folder (`Avatar (2009).mkv` next to
other movies) also work, but a folder per movie is what Plex recommends,
especially when you have subtitles, extras or a poster.

**Editions** (Director's Cut, Extended Edition, and so on) use the tag
`{edition-Name}` on the file and/or the folder. The name is ordinary
text, up to 32 characters. A Plex Pass is required for edition display.
Organimizer writes the tag on the **file** and keeps the folder as
`Title (Year)` (and `{tmdb-ID}` when you confirm a lookup). Plex also
allows the tag on the folder; that is useful if each edition has its
own extras.

    Blade Runner (1982)/
      Blade Runner (1982) {edition-Director's Cut}.mp4

**ID tags** help Plex match the title. On the current Plex Movie agent
the folder or file may include `{imdb-tt0372784}` or `{tmdb-272}` in
curly braces, in the form `{source-id}`. After a confirmed TMDB lookup,
Organimizer can add `{tmdb-ID}` to the folder (Tools > Options > TMDB;
on by default). It does not invent IMDb tags; you can add `{imdb-...}`
yourself if you want.

**Split / stacked files** (limited support in Plex; Plex recommends
joining the parts into one file when you can): put the parts in the
movie folder and name them with one of `cdX`, `discX`, `diskX`, `dvdX`,
`partX` or `ptX` (X is 1 to 8):

    The Dark Knight (2008)/
      The Dark Knight (2008) - pt1.mp4
      The Dark Knight (2008) - pt2.mp4

All parts should be the same format (all MP4, or all MKV). Organimizer
keeps a recognized split suffix on the new name.

**Multiple versions** of the *same* edition (1080p and 720p, or HEVC
and H.264) are named:

    Title (Year) - ArbitraryText.ext

for example `Pulp Fiction (1994) - 1080p.mkv` and
`Pulp Fiction (1994) - SD.m4v`. Plex merges them as one library item.
Editions (Director's Cut vs Theatrical) are different from versions.
Organimizer writes the clean `Title (Year).ext` name. If two files
would get the same name, the second gets ` (1)`, which is not Plex's
version syntax: add ` - 1080p` (or similar) yourself when you keep
more than one version.

**Subtitles** sit next to the video (or, from Plex Media Server 1.41,
in a `Subs` or `Subtitles` folder beside the video). Name them:

    Title (Year).en.srt
    Title (Year).eng.srt
    Title (Year).en.forced.srt
    Title (Year).en.sdh.srt
    Title (Year).en.cc.srt

The language code is ISO-639-1 (two letters) or ISO-639-2/B (three
letters). `forced` is for foreign-language lines only. `sdh` or `cc` is
for subtitles for the deaf and hard of hearing. Organimizer keeps
language / forced / sdh tags when it renames a subtitle with its
video.

### Movie extras

Plex accepts local trailers and extras in two ways. Enable "Use local
assets" (or Local Media Assets on a legacy agent) in the movie
library, and Refresh Metadata after you add extras.

**1. Official extras folders** inside the movie folder. The folder
names must be exactly these (English spelling):

  Behind The Scenes  -- how the film was made, making-of, BTS
  Deleted Scenes     -- scenes cut from the film
  Featurettes        -- short pieces about the film
  Interviews         -- interviews
  Scenes             -- individual scenes or clips
  Shorts             -- short films
  Trailers           -- trailers and teasers
  Other              -- anything else (gag reels, bloopers, ...)

Example:

    Avatar (2009)/
      Avatar (2009).mkv
      Behind The Scenes/
        Performance Capture.mkv
      Deleted Scenes/
        Bar Fight.mp4
      Interviews/
        Sigourney Weaver.mp4
      Scenes/
        Arrival.mp4
      Trailers/
        Teaser Trailer.mp4
        Theatrical Trailer #1.mp4

**2. Inline suffix**, in the same folder as the movie, with no space
around the hyphen. The filename must end in one of:

  -behindthescenes
  -deleted
  -featurette
  -interview
  -scene
  -short
  -trailer
  -other

Example: `Bar Fight-deleted.mp4`, `Teaser Trailer-trailer.mp4`. Plex
also allows a number index when you have more than one of the same
type: `Bar Fight-deleted1.mp4`. Organimizer recognizes those suffixes
and moves the files into the official extras folders above (the
subdirectory method).

### TV shows

Keep TV content in its own library folder, with one folder per show
and a `Season NN` folder (the English word "Season") for each season.
For the current Plex TV Series agent, Plex recommends including the
year in the show folder and the file name:

    TV Shows/
      Show (Year)/
        Season 01/
          Show (Year) - s01e01 - Optional Title.ext

The important part of the file name is `s01e01` (season and episode).
Dashes, dots or spaces around it are fine. An optional title after
that is a hint for matching; put junk you want Plex to ignore in
`[brackets]`.

**ID tags** on the show folder (Plex TV Series agent):
`{tmdb-123456}`, `{tvdb-123456}` or `{imdb-tt123456}`. Organimizer
can add `{tmdb-ID}` after a confirmed lookup.

**Specials** are always season zero. Put them in `Season 00` or
`Specials`, and name them `Show - s00e13 - Optional Title.ext`.
Organimizer writes `Season 00`. If a special is not a numbered
episode on TMDB (a gag reel, and so on), Plex's own docs say to add
it as a TV extra instead (see below).

**Several episodes in one file:** `Show - s01e01-e02 - Title.ext`
(first and last episode). Plex will list both episodes but play the
whole file.

**Date-based shows** (daily or talk shows): keep the Season folder,
and put the date in the file name as `YYYY-MM-DD` or `DD-MM-YYYY`
(dashes, dots or spaces):

    The Colbert Report (2005)/
      Season 08/
        The Colbert Report (2005) - 2011-11-15 - Elijah Wood.avi

If Organimizer can see the season folder, it uses that `Season NN`.
If it only has a date, it uses the year as a last-resort folder name
(`Season 2024`) - that is not Plex's official season number, so put
those files in the real season folder when you can.

**Episodes split across files:** the same tokens as movies (`pt1`,
`cd1`, and so on) after the episode name, for example
`Show - s01e01 - pt1.ext`. Plex recommends joining the parts when
you can. Organimizer keeps a recognized split suffix.

**Episode order and anime:** some series (especially anime) are
numbered as one long "absolute" list, and TMDB and The TVDB can
disagree on order. The Plex TV Series agent defaults to TMDB aired
order. In the library's Advanced settings you can choose TheTVDB
(Aired, DVD or Absolute) to match how your files are numbered.
Organimizer names files from the season and episode numbers it sees;
it does not convert absolute numbers into TMDB seasons. Name the
files the way they appear on the site you will use in Plex.

**TV extras** (Plex TV Series agent): the same eight folder names as
movies, either under the show folder or under a Season folder, or an
inline suffix next to an episode (`Show - s01e01 - Title-deleted.ext`,
and `-deleted1` if you have more than one). Organimizer puts show
extras in those official folders under the show (`Show/Featurettes/...`).
Not every Plex app shows TV extras yet.

**TV subtitles** follow the episode:

    Show - s02e03.en.srt
    Show - s02e03.en.forced.srt

A `Subs` or `Subtitles` folder inside the season folder is also
allowed (Plex Media Server 1.41+).

Example tree (shapes taken from Plex's own TV naming article):

    TV Shows/
      Doctor Who (1963)/
        Season 01/
          Doctor Who (1963) - s01e01 - An Unearthly Child (1).mp4
      Grey's Anatomy (2005)/
        Season 00/
          Grey's Anatomy (2005) - s00e01 - Straight to the Heart.mkv
        Season 01/
          Grey's Anatomy (2005) - s01e01 - pt1.avi
          Grey's Anatomy (2005) - s01e01 - pt2.avi
          Grey's Anatomy (2005) - s01e02 - The First Cut is the Deepest.avi
        Season 02/
          Grey's Anatomy (2005) - s02e01-e03.avi
      The Office (UK) (2001) {tmdb-2996}/
        Season 01/
          The Office (UK) - s01e01 - Downsize.mp4

### Common mistakes (and how Organimizer fixes them)

These before -> after paths are real cases from Organimizer's tests,
not made-up examples.

    Alien.1979.Directors.Cut.1080p.mkv
      -> Alien (1979)/Alien (1979) {edition-Director's Cut}.mkv

    The Expanse S2 E5.mkv
      -> The Expanse/Season 02/The Expanse - s02e05.mkv

    Expanse - 2x05 - Doors and Corners.mkv
      -> Expanse/Season 02/Expanse - s02e05 - Doors and Corners.mkv

    Lost S01E01E02.mkv
      -> Lost/Season 01/Lost - s01e01-e02.mkv

    TV/The Expanse/Season2/The Expanse S02E05.mkv
      -> The Expanse/Season 02/The Expanse - s02e05.mkv

    TV/The Expanse/Specials/The Expanse S00E01.mkv
      -> The Expanse/Season 00/The Expanse - s00e01.mkv

    alien making of.mkv   (next to the movie)
      -> Alien (1979)/Behind The Scenes/Making of.mkv

    Alien.1979.Trailer.mp4
      -> Alien (1979)/Trailers/Trailer.mp4

    Alien.1979.en.srt     (next to the movie)
      -> Alien (1979)/Alien (1979).en.srt

Always check the preview (and the Why column) before Apply.

### Cheat sheet

    Kind                 Plex name
    -------------------  ----------------------------------------------
    Movie                Title (Year)/Title (Year).ext
    Edition              Title (Year)/Title (Year) {edition-Name}.ext
    ID tag               Title (Year) {tmdb-123}/  or {imdb-tt...}
    Split movie          Title (Year)/Title (Year) - pt1.ext
    Two versions         Title (Year) - 1080p.ext  and  - SD.ext
    Subtitle             Title (Year).en.srt  (.forced / .sdh / .cc)
    Movie extra folder   Title (Year)/Trailers/Name.ext
                         (or Behind The Scenes, Deleted Scenes,
                         Featurettes, Interviews, Scenes, Shorts, Other)
    Movie extra suffix   Name-trailer.ext  (-behindthescenes, -deleted,
                         -featurette, -interview, -scene, -short, -other)
    TV episode           Show (Year)/Season 01/Show (Year) - s01e01 - Title.ext
    Special              Show (Year)/Season 00/Show (Year) - s00e01.ext
    Multi-episode        Show - s01e01-e02.ext
    Date-based           Show/Season 08/Show - 2011-11-15 - Title.ext
    Split episode        Show - s01e01 - pt1.ext
    TV extra             Show/Featurettes/Name.ext  (same eight folders)
    TV subtitle          Show - s01e01.en.srt

### Where these rules come from

Plex official support (retrieved for this Help text):

  https://support.plex.tv/articles/naming-and-organizing-your-movie-media-files/
  https://support.plex.tv/articles/local-files-for-trailers-and-extras/
  https://support.plex.tv/articles/naming-and-organizing-your-tv-show-files/
  https://support.plex.tv/articles/local-files-for-tv-show-trailers-and-extras/
  https://support.plex.tv/articles/200381043-multi-version-movies/
  https://support.plex.tv/articles/200471133-adding-local-subtitles-to-your-media/
  https://support.plex.tv/articles/multiple-editions-tv-shows/

## File types and protection


- *Images & videos* uses the list in Tools > Options (image, camera RAW and
  video types only).
- *Custom list*: type extensions in Tools > Options > Custom file types,
  for example `pdf, docx, mp3`.
- *All files*: every file that has an extension.
- **Always protected, in every mode:** programs, libraries, drivers,
  installers, scripts and shortcuts (`exe dll sys msi bat cmd ps1 vbs js
  lnk url reg inf ini` and similar), system files (`desktop.ini`,
  `thumbs.db`, `pagefile.sys`, ...), Organimizer's own settings and undo logs,
  hidden or system files, and folders such as `.git`, `$Recycle.Bin` and
  `System Volume Information`. Such files are never renamed or moved, and a
  rule can never change a file's extension to another type.

## Folder blacklist

**Tools > Folder Blacklist...** lists folders that can never be chosen and
are skipped when subfolders are scanned (both tabs). The built-in list is
worked out from this PC's Windows folders (Windows, Program Files,
ProgramData, the AppData folders, Recovery, `$Recycle.Bin`, drive roots
such as `C:\` ...). Add, edit or remove entries, test a folder, or click
**Reset to defaults**. Matching ignores letter case and works on whole
folder names: `C:\Windows` blocks `C:\Windows\Temp` but not
`C:\WindowsStuff`. `*` stands for one folder name
(`C:\Users\*\AppData`), `*:` for any drive (`*:\$Recycle.Bin`).
Choosing a blocked folder shows "Folder not allowed" and selects nothing.

## Favorites

- Click the star (outline) next to a folder field to add it to Favorites; it turns
  into a filled amber star. Click again to remove it.
- The **Favorites** menu jumps to a favorite (in the tab you are on).
- **Favorites > Manage Favorites...** (also in Tools): rename, change the
  folder, add, remove and reorder (Up / Down). Changes are saved at once.

## Getting a TMDB API key

The TMDB lookup on the Plex Media tab is **optional** and **off by
default**: without it everything is worked out offline from file and folder
names. To use it you need your own TMDB API key. It is **free** for
personal, non-commercial use and takes a few minutes:

1. **Create a free account** at <https://www.themoviedb.org/signup> and
   confirm your e-mail address. (Use a desktop browser: TMDB says the API
   pages are not made for phones.)
2. **Log in** and open your account **Settings** (click your profile
   picture at the top right, then *Settings*).
3. In the menu on the left click **API**
   (<https://www.themoviedb.org/settings/api>).
4. **Request an API key:** choose the **Developer** (personal use) option
   and **accept TMDB's terms of use**.
5. **Fill in the form** that follows: what you use it for (for example
   "Personal use: organizing my own movie files for Plex with PrimeCrew
   Organimizer"), an application name and URL (you can enter
   `https://www.themoviedb.org` if you have none), and your contact
   details. Submit it; the key is normally shown right away.
6. **Copy the key.** The API page now shows two values. Copy the long
   **API Read Access Token** (starts with `eyJ...`). The shorter **API Key**
   (32 letters and digits, "v3 auth") works too.
7. In Organimizer open **Tools > Options**, find **TMDB lookup**, tick
   **Look up movie and TV titles on TMDB**, **paste the key** into
   **Your TMDB key**, optionally set the language (for example `en-US` or
   `de-DE`), and click **Save**. The line under the field says whether the
   key looks right.

Good to know:

- The key is stored **only on your PC**, in `Organimizer.ini` (see
  "Portable or installed"), and it is sent only to TMDB
  (`api.themoviedb.org`) when you click **Look up**; poster thumbnails come
from `image.tmdb.org` without the key. Organimizer has
  no server of its own and sends nothing else. Do not share the INI file if
  you want to keep the key private.
- Lookups only suggest titles: you choose the right match, and nothing is
  moved until you click Apply.
- No internet, a wrong key or too many requests just show a message; the
  offline analysis keeps working. To stop using TMDB, untick the option or
  clear the key.
- This product uses the TMDB API but is not endorsed or certified by TMDB.

## Looking up titles on TMDB

Needs your own key (previous section) and the TMDB option switched on;
otherwise the **Look up** buttons are greyed out and their tooltip says what
to do.

1. **Scan** as usual. Above the list, **What you have** shows what
   Organimizer found, one line per movie or per TV season, for example
   `Looks like: Alien (1979) - 1 movie, 3 extras, 1 subtitle` or
   `Looks like: The Expanse - Season 2, 4 episodes`.
2. Click **Look up** on a line (or select a file in the list and click
   **Look up the selected file**). The results window shows the matches from
   TMDB with poster, title, year, original title, a short description and
   the TMDB number. If the filename is messy (confidence is not High) or
   TMDB finds nothing, a prompt asks you to type the name first (see
   below). If the right one is missing, change the title or year, switch
   between **Movie** and **TV show**, and click **Search again**, or click
   **Search for something else...**.
3. Click **This one** on the right match. The **match window** shows your
   files on the left and what TMDB has on the right:
   - a TV season: every file is matched to the TMDB episode with the same
     season and episode number (with its title and runtime);
   - a movie: the main file is matched to the movie; extras keep their own
     name and folder unless TMDB lists a video with a very similar name
     (TMDB videos are only name suggestions).
   Change a match with the drop-down list or the **Up** / **Down** buttons
   (two files never get the same episode: they swap), or untick a file to
   leave it as it is.
4. Click **OK - use these names**. The names go into the preview, marked
   **TMDB** in the Match column, with "Confirmed on TMDB" in the Why column.
   Nothing is moved yet: check the list and click **Apply** as usual
   (never overwrites, **Undo last run** puts everything back).

### If it can't find your show

Organimizer asks you to type the movie or TV show name when the filename
confidence is not High (Medium or Low: the name may be too messy for a
good TMDB search), or when TMDB returns no matches. You can also click
**Search for something else...** at any time in the results window.

The box is pre-filled with the best guess from the filename or folder, and
Movie / TV show is pre-filled from what was detected. Type the real name,
optionally a 4-digit year (1870 through next year; leave the year empty if
you are not sure), and press Enter or click **Search**. Esc or **Cancel**
closes the prompt and leaves the current preview unchanged.

The search then shows the usual results window. Choosing a match still goes
through the preview, then **Apply**, and **Undo last run** as usual: nothing
is renamed from this prompt.

A name you type is remembered for that folder until you close the app, so
Look up again uses it without asking first (you can still search for
something else).

Good to know:

- **Tools > Options > TMDB > Add {tmdb-ID} to the folder name** (on by
  default) names the folder like `Alien (1979) {tmdb-348}`, so Plex uses
  exactly that title instead of guessing.
- Titles and episode names come in the language set in Options (for example
  `en-US`, `de-DE`).
- Each search is asked only once while the app is open; Organimizer also
  waits a little between requests and tries once more if TMDB says "too
  many requests". Posters are small thumbnails from `image.tmdb.org` and are
  simply left out if they cannot be loaded.
- No internet or a wrong key shows a plain message in the window; nothing
  else changes.

## Portable or installed

- **Portable zip:** settings are kept in `Organimizer.ini` next to
  `Organimizer.exe`.
- **Installed with setup:** settings are kept in
  `%APPDATA%\PrimeCrew Software\Organimizer\Organimizer.ini`; the Start
  Menu shortcut is *PrimeCrew Software > Organimizer*.
- Either way, undo logs (`Organimizer-undo-*.json`) are written into the
  folder you sorted or scanned.

## Coming from PhotoSorter

- On the first start Organimizer imports your PhotoSorter settings and
  presets (`PhotoSorter.ini` next to the exe, or
  `%APPDATA%\PrimeCrew Software\PhotoSorter\PhotoSorter.ini`) into the new
  `Organimizer.ini`. The old file is left exactly as it is.
- Setup recognizes an installed PhotoSorter: it shows "Updating existing
  installation: PhotoSorter v2.0.5 -> Organimizer v3.0.6",
  installs Organimizer next to it, then removes the old PhotoSorter program,
  its Start Menu shortcut and its Add/Remove Programs entry. Your files,
  settings and undo logs are kept.
- **Undo last run** also finds undo logs written by PhotoSorter.

## Safety

- Only the file types you choose are touched, and protected files never.
- Nothing is overwritten or deleted. Name clashes get ` (1)`, ` (2)`, ...
- The preview always comes first, and every Apply can be undone.

## Troubleshooting

- **Windows SmartScreen:** the exe is not code-signed, so Windows may show
  "Windows protected your PC" on first run. Click **More info > Run
  anyway** after checking the SHA-256 published with the release.
- **"Folder not allowed":** the folder is on the blacklist (Tools > Folder
  Blacklist).
- **TMDB says the key is not valid (401):** copy the key again from
  <https://www.themoviedb.org/settings/api> and paste it into Tools >
  Options.
- **A movie is detected as something else:** click its name in the Plex
  table and correct the type and title, or untick it.

## Keyboard shortcuts

| Key    | Action        |
|--------|---------------|
| Ctrl+N | New Project   |
| Ctrl+O | Open Folder...  |
| F1     | This help (on the Plex tab, start at the Contents line "Plex tab") |

## About and license

Copyright (C) 2026 PrimeCrew Software. Licensed under the GNU General
Public License v3.0 or later. See `LICENSE` and `COPYRIGHT` next to `Organimizer.exe`.
Third-party licenses: `THIRD-PARTY-NOTICES.txt`.

This product uses the TMDB API but is not endorsed or certified by TMDB.
