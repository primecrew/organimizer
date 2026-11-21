//! "Almost named right" fixtures: the title and numbers are there, the
//! syntax is not what Plex wants. Each case is the input path (relative to a
//! library root) and the exact Plex path the app should suggest.

use organimizer::plex::{analyze_files, relative_path, Kind, Row};
use std::path::{Path, PathBuf};

fn plex_paths(files: &[&str]) -> Vec<(String, String, Kind)> {
    let root = PathBuf::from("/lib");
    let paths: Vec<PathBuf> = files.iter().map(|f| root.join(f)).collect();
    let rows = analyze_files(&root, &paths);
    rows.iter()
        .map(|r| {
            let out = match r.subtitle_of {
                Some(v) => format!("subtitle of {}", out_of(&rows[v])),
                None => out_of(r),
            };
            (input_of(&root, r), out, r.analysis.kind)
        })
        .collect()
}

fn input_of(root: &Path, r: &Row) -> String {
    r.source.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/")
}

fn out_of(r: &Row) -> String {
    let name = r.source.file_name().unwrap().to_string_lossy().into_owned();
    let ext = name.rfind('.').map(|i| name[i..].to_string()).unwrap_or_default();
    relative_path(&r.analysis, &ext)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| format!("<incomplete: {}>", r.analysis.why_text()))
}

/// Single files, analysed on their own.
const SINGLE: &[(&str, &str)] = &[
    // --- movies: separators, year without parentheses, junk tags
    ("Alien.1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien 1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien_1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien-1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien [1979].mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien(1979).mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien - 1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("alien.1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("ALIEN 1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien.1979.1080p.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien.1979.1080p.BluRay.x264-GROUP.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien (1979) 1080p.mp4", "Alien (1979)/Alien (1979).mp4"),
    ("Alien (1979) [1080p].mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Alien.(1979).mkv", "Alien (1979)/Alien (1979).mkv"),
    ("The.Thing.1982.mkv", "The Thing (1982)/The Thing (1982).mkv"),
    ("the_thing_1982_720p.mkv", "The Thing (1982)/The Thing (1982).mkv"),
    ("Back.to.the.Future.1985.mkv", "Back to the Future (1985)/Back to the Future (1985).mkv"),
    ("Blade Runner 2049 (2017).mkv", "Blade Runner 2049 (2017)/Blade Runner 2049 (2017).mkv"),
    ("Blade.Runner.2049.2017.mkv", "Blade Runner 2049 (2017)/Blade Runner 2049 (2017).mkv"),
    ("1917.2019.mkv", "1917 (2019)/1917 (2019).mkv"),
    ("Movies/Alien 1979/Alien 1979.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Movies/Alien (1979)/alien.mkv", "Alien (1979)/Alien (1979).mkv"),
    ("Movies/Alien.1979.1080p/Alien.1979.1080p.mkv", "Alien (1979)/Alien (1979).mkv"),
    // --- editions
    ("Alien.1979.Directors.Cut.1080p.mkv", "Alien (1979)/Alien (1979) {edition-Director's Cut}.mkv"),
    ("Alien 1979 Director's Cut.mkv", "Alien (1979)/Alien (1979) {edition-Director's Cut}.mkv"),
    ("Alien (1979) - Directors Cut.mkv", "Alien (1979)/Alien (1979) {edition-Director's Cut}.mkv"),
    ("Aliens.1986.Extended.mkv", "Aliens (1986)/Aliens (1986) {edition-Extended Edition}.mkv"),
    ("Aliens (1986) Special Edition.mkv", "Aliens (1986)/Aliens (1986) {edition-Special Edition}.mkv"),
    ("Blade.Runner.1982.Final.Cut.mkv", "Blade Runner (1982)/Blade Runner (1982) {edition-Final Cut}.mkv"),
    ("Apocalypse.Now.1979.Theatrical.mkv", "Apocalypse Now (1979)/Apocalypse Now (1979) {edition-Theatrical Cut}.mkv"),
    // --- TV: SxxEyy variants
    ("The Expanse S2 E5.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse S2E5.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The.Expanse.s02e05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse - S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse S02 E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse S02.E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The_Expanse_S02E05_Doors_and_Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
    ("The Expanse S02E05 - Doors and Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
    ("Expanse - 2x05 - Doors and Corners.mkv", "Expanse/Season 02/Expanse - s02e05 - Doors and Corners.mkv"),
    ("Expanse 2x05.mkv", "Expanse/Season 02/Expanse - s02e05.mkv"),
    ("Expanse.2x05.Doors.and.Corners.mkv", "Expanse/Season 02/Expanse - s02e05 - Doors and Corners.mkv"),
    ("Expanse - 02x05.mkv", "Expanse/Season 02/Expanse - s02e05.mkv"),
    ("The Expanse Season 2 Episode 5.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse - Season 2 - Episode 5 - Doors and Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
    ("The Expanse (2015) S02E05.mkv", "The Expanse (2015)/Season 02/The Expanse (2015) - s02e05.mkv"),
    ("The.Expanse.2015.S02E05.mkv", "The Expanse (2015)/Season 02/The Expanse (2015) - s02e05.mkv"),
    ("the expanse s02e05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("The Expanse S10E105.mkv", "The Expanse/Season 10/The Expanse - s10e105.mkv"),
    ("Lost S01E01E02.mkv", "Lost/Season 01/Lost - s01e01-e02.mkv"),
    ("Lost.S1E1-2.mkv", "Lost/Season 01/Lost - s01e01-e02.mkv"),
    // --- TV: folders
    ("TV/The Expanse/Season2/The Expanse S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season 2/The Expanse S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/S2/The Expanse S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/S02/05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season 02/E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season 2/Episode 5.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season 2/5 - Doors and Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
    ("TV/The Expanse/Season 2/05. Doors and Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
    ("TV/The Expanse/Season 2/s02e05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season 2/Expanse 2x05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Season_2/The.Expanse.S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/season 2/the expanse s2e5.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The Expanse/Specials/The Expanse S00E01.mkv", "The Expanse/Season 00/The Expanse - s00e01.mkv"),
    ("TV/The Expanse - Season 2/The Expanse S02E05.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    ("TV/The.Expanse.S02.1080p/The.Expanse.S02E05.1080p.mkv", "The Expanse/Season 02/The Expanse - s02e05.mkv"),
    // --- more near misses
    ("Breaking Bad S1E1.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01.mkv"),
    ("Breaking.Bad.s1.e1.Pilot.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01 - Pilot.mkv"),
    ("breaking_bad_1x01_pilot.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01 - Pilot.mkv"),
    ("Breaking Bad - 1x01 - Pilot (720p).mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01 - Pilot.mkv"),
    ("Breaking Bad [S01E01] Pilot.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01 - Pilot.mkv"),
    ("Breaking Bad (S01E01).mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01.mkv"),
    ("Breaking Bad S01E01 Pilot HDTV x264.mkv", "Breaking Bad/Season 01/Breaking Bad - s01e01 - Pilot.mkv"),
    ("Doctor Who (2005) - 1x01 - Rose.mkv", "Doctor Who (2005)/Season 01/Doctor Who (2005) - s01e01 - Rose.mkv"),
    ("Doctor.Who.2005.1x01.Rose.mkv", "Doctor Who (2005)/Season 01/Doctor Who (2005) - s01e01 - Rose.mkv"),
    ("TV/Doctor Who (2005)/Season 1/Doctor Who 1x01 Rose.mkv", "Doctor Who (2005)/Season 01/Doctor Who (2005) - s01e01 - Rose.mkv"),
    ("TV/Doctor Who 2005/Season 01/Doctor.Who.S01E01.mkv", "Doctor Who (2005)/Season 01/Doctor Who (2005) - s01e01.mkv"),
    ("TV/Fargo/Season 3/Fargo.S03E01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Fargo Season 3/Fargo.S03E01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Series 3/Fargo.S03E01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Staffel 3/Fargo.S03E01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/S03/Fargo - 3x01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Season 3/Fargo - 01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Season 3/Fargo Ep01.mkv", "Fargo/Season 03/Fargo - s03e01.mkv"),
    ("TV/Fargo/Season 3/Fargo Ep 1 The Law of Vacant Places.mkv", "Fargo/Season 03/Fargo - s03e01 - The Law of Vacant Places.mkv"),
    ("Movies/Inception/Inception.2010.mkv", "Inception (2010)/Inception (2010).mkv"),
    ("Movies/Inception 2010/Inception.mkv", "Inception (2010)/Inception (2010).mkv"),
    ("Movies/Inception [2010]/Inception [2010].mkv", "Inception (2010)/Inception (2010).mkv"),
    ("Movies/Inception (2010)/Inception (2010).mkv", "Inception (2010)/Inception (2010).mkv"),
    ("Inception 2010 BluRay 1080p DTS.mkv", "Inception (2010)/Inception (2010).mkv"),
    ("Inception.2010.REMASTERED.1080p.mkv", "Inception (2010)/Inception (2010) {edition-Remastered}.mkv"),
    ("Star.Wars.Episode.IV.A.New.Hope.1977.mkv", "Star Wars Episode IV A New Hope (1977)/Star Wars Episode IV A New Hope (1977).mkv"),
    ("Spider-Man.2002.mkv", "Spider-Man (2002)/Spider-Man (2002).mkv"),
    ("Mission Impossible - Fallout 2018.mkv", "Mission Impossible - Fallout (2018)/Mission Impossible - Fallout (2018).mkv"),
    ("WALL-E.2008.mkv", "WALL-E (2008)/WALL-E (2008).mkv"),
    // --- subtitles keep their language
    ("Alien.1979.en.srt", "<subtitle>"),
];

/// Files in one folder, analysed together (extras next to their movie).
const GROUPS: &[&[(&str, &str)]] = &[
    &[
        ("Alien (1979).mkv", "Alien (1979)/Alien (1979).mkv"),
        ("alien making of.mkv", "Alien (1979)/Behind The Scenes/Making of.mkv"),
        ("Alien (1979) - Deleted Scenes.mp4", "Alien (1979)/Deleted Scenes/Deleted Scenes.mp4"),
        ("Alien.1979.Trailer.mp4", "Alien (1979)/Trailers/Trailer.mp4"),
        ("Alien (1979) Featurette - The Beast Within.mkv", "Alien (1979)/Featurettes/The Beast Within.mkv"),
        ("Alien 1979 interview with Ridley Scott.mkv", "Alien (1979)/Interviews/Interview with Ridley Scott.mkv"),
        ("Alien.1979.en.srt", "subtitle of Alien (1979)/Alien (1979).mkv"),
    ],
    &[
        ("Movies/Heat.1995.1080p/Heat.1995.1080p.mkv", "Heat (1995)/Heat (1995).mkv"),
        ("Movies/Heat.1995.1080p/Extras/Behind the Scenes.mkv", "Heat (1995)/Behind The Scenes/Behind the Scenes.mkv"),
        ("Movies/Heat.1995.1080p/Featurettes/Return to the Scene.mkv", "Heat (1995)/Featurettes/Return to the Scene.mkv"),
        ("Movies/Heat.1995.1080p/Deleted Scenes/Diner.mkv", "Heat (1995)/Deleted Scenes/Diner.mkv"),
    ],
    &[
        ("Inception.2010.1080p.mkv", "Inception (2010)/Inception (2010).mkv"),
        ("Inception.2010.1080p.en.srt", "subtitle of Inception (2010)/Inception (2010).mkv"),
        ("inception trailer.mp4", "Inception (2010)/Trailers/Trailer.mp4"),
        ("Inception - Behind The Scenes.mkv", "Inception (2010)/Behind The Scenes/Behind The Scenes.mkv"),
        ("Inception (2010) Deleted Scene - Kitchen.mkv", "Inception (2010)/Deleted Scenes/Kitchen.mkv"),
        ("Inception.Featurette.Dreams.mkv", "Inception (2010)/Featurettes/Featurette Dreams.mkv"),
    ],
    &[
        ("TV/The Expanse/Season2/Expanse - 2x05 - Doors and Corners.mkv", "The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
        ("TV/The Expanse/Season2/Expanse - 2x05 - Doors and Corners.en.srt", "subtitle of The Expanse/Season 02/The Expanse - s02e05 - Doors and Corners.mkv"),
        ("TV/The Expanse/Season2/Expanse - 2x06 - Paradigm Shift.mkv", "The Expanse/Season 02/The Expanse - s02e06 - Paradigm Shift.mkv"),
        ("TV/The Expanse/Featurettes/Inside the Expanse.mkv", "The Expanse/Featurettes/Inside the Expanse.mkv"),
    ],
];

#[test]
fn near_miss_single_files() {
    let mut failures = vec![];
    for (input, want) in SINGLE {
        if *want == "<subtitle>" {
            continue;
        }
        let got = plex_paths(&[input]);
        let (_, out, _) = &got[0];
        if out != want {
            failures.push(format!("{input}\n    want {want}\n    got  {out}"));
        }
    }
    assert!(failures.is_empty(), "{} of {} wrong:\n{}", failures.len(), SINGLE.len(), failures.join("\n"));
}

#[test]
fn near_miss_groups() {
    let mut failures = vec![];
    for group in GROUPS {
        let inputs: Vec<&str> = group.iter().map(|(i, _)| *i).collect();
        let got = plex_paths(&inputs);
        for (input, want) in group.iter() {
            let (_, out, _) = got.iter().find(|(i, _, _)| i == input).unwrap();
            if out != want {
                failures.push(format!("{input}\n    want {want}\n    got  {out}"));
            }
        }
    }
    assert!(failures.is_empty(), "{} wrong:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn enough_fixtures() {
    let n = SINGLE.len() + GROUPS.iter().map(|g| g.len()).sum::<usize>();
    assert!(n >= 60, "{n}");
}

#[test]
fn every_row_explains_itself() {
    let root = PathBuf::from("/lib");
    let mut all: Vec<&str> = SINGLE.iter().map(|(i, _)| *i).collect();
    all.extend(GROUPS.iter().flat_map(|g| g.iter().map(|(i, _)| *i)));
    for f in all {
        let rows = analyze_files(&root, &[root.join(f)]);
        let r = &rows[0];
        if r.subtitle_of.is_none() && r.analysis.kind != Kind::Skip && !f.ends_with(".srt") {
            assert!(!r.analysis.why_text().is_empty(), "no Why for {f}");
        }
    }
}
