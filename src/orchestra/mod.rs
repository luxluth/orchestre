use std::{
    fs::{self, DirBuilder},
    io::{Read, Write},
    path::PathBuf,
    time::{Duration, SystemTime},
};

use glob::glob;
use lofty::{
    file::{AudioFile, TaggedFileExt},
    picture::PictureType,
    probe::Probe,
    tag::{Accessor, ItemKey},
};

use mtk::windowing::WindowHandle;
use track::{Album, Artist, Cover, Id, IdKey, IdStore, MusicCollection, Song, Timestamp};

use crate::orchestra::mu_thread::{AppMsg, OrchestraMsg};

pub mod audio;
pub mod di;
pub mod kmeans;
pub mod mu_thread;
pub mod source;
pub mod track;

pub struct Utils;

impl Utils {
    pub fn cache_dir() -> PathBuf {
        let cache_dir = dirs::cache_dir().unwrap().join("lorchestre");
        if !cache_dir.exists() {
            let _ = DirBuilder::new()
                .recursive(true)
                .create(cache_dir.as_path());
        }

        cache_dir
    }

    pub fn cache_path() -> PathBuf {
        Utils::cache_dir().join("_index")
    }

    pub fn covers_dir() -> PathBuf {
        let store = Utils::cache_dir().join("__COVERS_STORE");
        if !store.exists() {
            let _ = DirBuilder::new().recursive(true).create(store.as_path());
        }

        store
    }
}

#[derive(Debug, Clone)]
pub struct Orchestra {
    pub collection: MusicCollection,

    artist_id_store: IdStore,
    album_id_store: IdStore,
    song_id_store: IdStore,

    __default_cover: Cover,
}

impl Orchestra {
    pub fn new() -> Self {
        Self {
            collection: MusicCollection::new(),
            artist_id_store: IdStore::default(),
            album_id_store: IdStore::default(),
            song_id_store: IdStore::default(),

            __default_cover: Cover::new(Id::Unresolved, &lofty::picture::MimeType::Png),
        }
    }

    pub fn load_from_cache(&mut self) -> bool {
        self.save_or_load_default_cover();
        let cache_path = Utils::cache_path();
        if let Ok(mut f) = std::fs::File::open(cache_path) {
            let config = bincode::config::standard();
            let mut buf = Vec::new();
            if f.read_to_end(&mut buf).is_ok() {
                match bincode::decode_from_slice(&buf, config) {
                    Ok((collection, _)) => {
                        self.collection = collection;
                        return true;
                    }
                    Err(_) => {
                        self.collection = MusicCollection::new();
                        return false;
                    }
                }
            } else {
                return false;
            }
        } else {
            return false;
        }
    }

    pub fn save(&mut self) {
        let cache_path = Utils::cache_path();
        let mut f = std::fs::File::create(cache_path).unwrap();
        let config = bincode::config::standard();
        let data = bincode::encode_to_vec(&self.collection, config).unwrap();
        f.write_all(&data).unwrap();
    }
}

impl Orchestra {
    pub fn index_file(&mut self, file_path: PathBuf) {
        let id = self
            .song_id_store
            .digest(format!("{}", file_path.to_str().unwrap()).as_bytes());

        // eprintln!("..index@{id:?} :: {file_path:?}");

        if let Ok(tagged_file) = Probe::open(&file_path).unwrap().read() {
            let properties = tagged_file.properties();
            let bitrate = properties.audio_bitrate().unwrap_or(0);
            let duration = properties.duration();
            // let mime = tagged_file.file_type();

            let default_tag = lofty::tag::Tag::new(lofty::tag::TagType::Id3v2);
            let tag = match tagged_file.primary_tag() {
                Some(primary_tag) => primary_tag,
                // If the "primary" tag doesn't exist, we just grab the
                // first tag we can find. Realistically, a tag reader would likely
                // iterate through the tags to find a suitable one.
                None => tagged_file.first_tag().unwrap_or(&default_tag),
            };

            let mut audio = Song::new(id, file_path.clone());

            audio.bitrate = bitrate;
            audio.duration = duration;

            if let Ok(meta) = file_path.metadata() {
                if let Ok(tm) = meta.created() {
                    audio.created_at = tm
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or(Duration::default())
                        .as_secs();
                }
            };

            if let Some(encoder) = tag.get_string(ItemKey::EncoderSettings) {
                audio.encoder = encoder.to_string();
            }

            if let Some(title) = tag.title() {
                audio.title = title.to_string();
                self.collection.index.insert(title, IdKey::SongTitle(id));
            }

            if let Some(artists) = tag.get_string(ItemKey::TrackArtist) {
                let spliter = artists
                    .contains('/')
                    .then_some(Some('/'))
                    .unwrap_or(Some(';'))
                    .unwrap();
                audio.artists = artists
                    .split(spliter)
                    .filter(|x| !x.is_empty())
                    .map(|x| self.add_artist(x.trim().to_string()))
                    .collect();
            };

            audio.disc = tag.disk().unwrap_or(1);
            if let Some(no) = tag.track() {
                audio.track = no;
            }

            if let Some(lyrics) = tag.get_string(ItemKey::Lyrics) {
                audio.embeded_lyrics = Some(lyrics.to_string());
            }

            if let Some(album) = tag.album() {
                let mut bytes = album.as_bytes().to_vec();
                let album_artist = {
                    if let Some(album_artist) = tag.get_string(ItemKey::AlbumArtist) {
                        album_artist
                    } else {
                        if let Some(id) = audio.artists.first() {
                            self.get_artist(id).unwrap().name.as_str()
                        } else {
                            "@UNKOWN@"
                        }
                    }
                }
                .as_bytes();
                bytes.extend(album_artist);

                let album_id = self.album_id_store.digest(bytes);
                if let Some(album) = self.collection.albums.get_mut(&album_id) {
                    audio.album = Some(album_id);
                    if let Some(genres) = tag.genre() {
                        if genres.contains(';') {
                            let genres: Vec<String> = genres
                                .split(';')
                                .filter_map(|x| {
                                    let g = x.trim().to_string();
                                    if g.is_empty() { None } else { Some(g) }
                                })
                                .collect();
                            album.genres.extend(genres);
                        } else {
                            let genres: Vec<String> = genres
                                .split(' ')
                                .filter_map(|x| {
                                    let g = x.trim().to_string();
                                    if g.is_empty() { None } else { Some(g) }
                                })
                                .collect();
                            album.genres.extend(genres);
                        }
                    }

                    album.songs.push(audio.id);
                } else {
                    let mut album = Album::new(album_id);
                    audio.album = Some(album_id);

                    if let Some(genres) = tag.genre() {
                        if genres.contains(';') {
                            let genres: Vec<String> =
                                genres.split(';').map(|x| x.trim().to_string()).collect();
                            album.genres.extend(genres);
                        } else {
                            let genres: Vec<String> =
                                genres.split(' ').map(|x| x.trim().to_string()).collect();
                            album.genres.extend(genres);
                        }
                    }

                    if let Some(album_name) = tag.album() {
                        album.name = album_name.to_string();
                        self.collection
                            .index
                            .insert(&album_name, IdKey::AlbumName(album_id));
                    }

                    album.artist = {
                        if let Some(album_artist) = tag.get_string(ItemKey::OriginalArtist) {
                            self.get_artist_id_by_name(album_artist)
                        } else {
                            if let Some(id) = audio.artists.first() {
                                Some(*id)
                            } else {
                                self.get_artist_id_by_name("@UNKOWN@")
                            }
                        }
                    };

                    if let Some(date) = tag.date() {
                        album.date = Some(Timestamp::from_lofty(date));
                    }

                    album.songs.push(audio.id);
                    if let Some(tt) = tag.track_total() {
                        album.songs_count = tt;
                    }

                    album.disc_total = tag.disk_total().unwrap_or(1);
                    let cover_id = album.id;

                    let possible_covers = [
                        PictureType::CoverFront,
                        PictureType::Media,
                        PictureType::Other,
                        PictureType::CoverBack,
                    ];

                    'cover_loop: for picture_type in possible_covers {
                        if let Some(cover) = tag.get_picture_type(picture_type) {
                            let mime = cover.mime_type().unwrap();
                            let data = cover.data().to_vec();

                            let mut cover = Cover::new(cover_id, mime);
                            let cover_path = cover.get_path();

                            let mut f = fs::File::create(&cover_path).unwrap();
                            f.write_all(&data).unwrap();

                            let swatches =
                                kmeans::extract_album_palette(cover_path, 5).unwrap_or(Vec::new());
                            cover.swatches = swatches;

                            album.cover = Some(cover.id);
                            self.collection.covers.insert(cover_id, cover);

                            break 'cover_loop;
                        }
                    }

                    self.collection.albums.insert(album_id, album);
                }
            }

            self.collection.songs.insert(id, audio);
        }
    }

    pub fn add_artist(&mut self, name: String) -> Id {
        for (id, artist) in &self.collection.artists {
            if artist.name == name {
                return *id;
            }
        }

        let id = self.artist_id_store.next();
        self.collection.index.insert(&name, IdKey::ArtistName(id));
        let artist = Artist { id, name };

        self.collection.artists.insert(id, artist);

        return id;
    }

    pub fn get_artist_id_by_name(&self, name: &str) -> Option<Id> {
        for (id, artist) in &self.collection.artists {
            if artist.name == name {
                return Some(*id);
            }
        }

        None
    }

    pub fn get_artist(&self, id: &Id) -> Option<&Artist> {
        self.collection.artists.get(id)
    }

    pub fn get_album(&self, id: &Id) -> Option<&Album> {
        self.collection.albums.get(id)
    }

    pub fn get_cover(&self, id: &Id) -> Option<&Cover> {
        self.collection
            .covers
            .get(id)
            .or(Some(&self.__default_cover))
    }

    pub fn get_song(&self, id: &Id) -> Option<&Song> {
        self.collection.songs.get(id)
    }

    pub fn index(&mut self, dir_path: PathBuf, h: &WindowHandle<AppMsg>) {
        self.add_artist("@UNKOWN@".to_string());
        if let Ok(paths) = glob(&format!("{}/**/*", dir_path.display())) {
            for inode in paths.flatten() {
                if inode.is_file() {
                    let guess =
                        mime_guess::from_path(&inode).first_or("text/plain".parse().unwrap());
                    if guess.type_() == mime_guess::mime::AUDIO {
                        let _ = h.send(AppMsg::Orchestra(OrchestraMsg::Indexing(inode.clone())));
                        self.index_file(inode);
                    }
                }
            }
        }

        self.save_or_load_default_cover();
        self.collection.index.finalize();
    }

    pub fn save_or_load_default_cover(&mut self) {
        let mut cover = Cover::new(Id::Unresolved, &lofty::picture::MimeType::Png);
        let cover_path = cover.get_path();

        if cover_path.exists() {
            let swatches = kmeans::extract_album_palette(cover_path, 5).unwrap_or(Vec::new());
            cover.swatches = swatches;

            self.__default_cover = cover;

            return;
        }

        let mut f = fs::File::create(&cover_path).unwrap();
        f.write_all(IMAGE_FRAME_39).unwrap();

        let swatches = kmeans::extract_album_palette(cover_path, 5).unwrap_or(Vec::new());
        cover.swatches = swatches;

        self.__default_cover = cover;
    }
}

pub const IMAGE_FRAME_39: &[u8] = include_bytes!("../assets/images/Frame 39.png");
