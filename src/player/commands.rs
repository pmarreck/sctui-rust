use crate::api::Track;

pub enum PlayerCommand {
    Play(Track),
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
