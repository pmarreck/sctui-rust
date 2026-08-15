use crate::api::Track;

pub enum PlayerCommand {
    Play(Track),
    #[allow(dead_code)]
    PlayFromPosition(Track, u64),
    PreloadNext(Track),
    Pause,
    Resume,
    VolumeUp,
    VolumeDown,
    #[allow(dead_code)]
    NextSong,
    #[allow(dead_code)]
    PrevSong,
    FastForward,
    Rewind,
}
