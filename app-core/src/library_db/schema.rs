diesel::table! {
    library_meta (id) {
        id -> BigInt,
        folder -> Text,
        scan_count -> BigInt,
    }
}

diesel::table! {
    songs (id) {
        id -> BigInt,
        path -> Text,
        file_hash -> Text,
        title -> Text,
        artist -> Text,
        album -> Text,
        genre -> Text,
        duration_secs -> Double,
        album_art_path -> Nullable<Text>,
        is_analyzed -> Bool,
        language -> Nullable<Text>,
        transcript_source -> Nullable<Text>,
        is_video -> Bool,
        has_lrc_file -> Bool,
        has_embedded_lyrics -> Bool,
        payload -> Text,
    }
}

diesel::table! {
    analysis_queue (file_hash) {
        file_hash -> Text,
        rowid -> BigInt,
        status -> Text,
        analyzing_pct -> Nullable<BigInt>,
        failed_message -> Nullable<Text>,
        failed_kind -> Nullable<Text>,
        failed_acknowledged -> Bool,
    }
}

diesel::table! {
    analysis_timings (id) {
        id -> BigInt,
        file_hash -> Text,
        started_at -> Text,
        device -> Nullable<Text>,
        whisper_model -> Text,
        beam_size -> BigInt,
        batch_size -> BigInt,
        separator -> Text,
        asr_engine -> Text,
        align_backend -> Text,
        vocal_detection_threshold_pct -> Double,
        key_detect_ms -> Nullable<BigInt>,
        separation_ms -> Nullable<BigInt>,
        transcribe_or_align_ms -> Nullable<BigInt>,
        transcribe_ms -> Nullable<BigInt>,
        align_ms -> Nullable<BigInt>,
        load_avg_1m -> Nullable<Double>,
        gpu_active_ratio -> Nullable<Double>,
        gpu_freq_mhz -> Nullable<BigInt>,
        gpu_temp_c -> Nullable<Double>,
        cpu_active_ratio -> Nullable<Double>,
        mem_pressure_ratio -> Nullable<Double>,
        total_ms -> BigInt,
    }
}

diesel::table! {
    playlists (id) {
        id -> Text,
        name -> Text,
    }
}

diesel::table! {
    playlist_songs (playlist_id, song_id) {
        playlist_id -> Text,
        song_id -> BigInt,
        position -> BigInt,
    }
}

diesel::joinable!(playlist_songs -> playlists (playlist_id));
diesel::joinable!(playlist_songs -> songs (song_id));

diesel::allow_tables_to_appear_in_same_query!(
    library_meta,
    songs,
    analysis_queue,
    analysis_timings,
    playlists,
    playlist_songs,
);
