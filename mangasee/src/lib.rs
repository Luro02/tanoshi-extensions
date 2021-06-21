use std::error::Error;

use chrono::NaiveDateTime;
use fancy_regex::Regex;
use rayon::prelude::*;
use serde::de::Deserializer;
use serde::de::{self, Unexpected};
use serde::Deserialize;
use tanoshi_lib::prelude::*;

pub static ID: i64 = 3;
pub static NAME: &str = "mangasee";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Dir {
    /// Link
    pub i: String,
    /// Title
    pub s: String,
    /// Official translation
    pub o: String,
    /// Scan status
    pub ss: String,
    /// Publish status
    pub ps: String,
    /// Type
    pub t: String,
    /// View ?
    pub v: String,
    /// vm
    pub vm: String,
    /// Year of published
    pub y: String,
    /// Authors
    pub a: Vec<String>,
    /// Alternative names
    pub al: Vec<String>,
    /// Latest
    pub l: String,
    /// Last chapter
    pub lt: i64,
    /// Last chapter
    #[serde(deserialize_with = "date_or_zero")]
    pub ls: NaiveDateTime,
    /// Genres
    pub g: Vec<String>,
    /// Hentai?
    pub h: bool,
}

struct DateOrZeroVisitor;

impl<'de> de::Visitor<'de> for DateOrZeroVisitor {
    type Value = NaiveDateTime;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("an integer or a string")
    }

    fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(NaiveDateTime::from_timestamp(v as i64, 0))
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if let Ok(dt) = NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S%z") {
            Ok(dt)
        } else {
            Err(E::invalid_value(Unexpected::Str(v), &self))
        }
    }
}

fn date_or_zero<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(DateOrZeroVisitor)
}

impl Into<Manga> for &Dir {
    fn into(self) -> Manga {
        Manga {
            source_id: ID,
            title: self.s.clone(),
            author: self.a.clone(),
            genre: self.g.clone(),
            status: Some(self.ss.clone()),
            description: None,
            path: format!("/manga/{}", self.i),
            cover_url: format!("https://cover.nep.li/cover/{}.jpg", self.i),
        }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirChapter {
    #[serde(skip)]
    pub index_name: String,
    #[serde(rename(deserialize = "Chapter"))]
    pub chapter: String,
    #[serde(rename(deserialize = "Type"))]
    pub type_field: String,
    #[serde(rename(deserialize = "Date"), deserialize_with = "parse_date")]
    pub date: NaiveDateTime,
    #[serde(rename(deserialize = "ChapterName"))]
    pub chapter_name: Option<String>,
    #[serde(rename(deserialize = "Page"))]
    pub page: Option<String>,
}

struct DateVisitor;

impl<'de> de::Visitor<'de> for DateVisitor {
    type Value = NaiveDateTime;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        if let Ok(dt) = NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S") {
            Ok(dt)
        } else {
            Err(E::invalid_value(Unexpected::Str(v), &self))
        }
    }
}

fn parse_date<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_any(DateVisitor)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CurChapter {
    pub chapter: String,
    #[serde(rename = "Type")]
    pub chapter_type: String,
    pub page: String,
    pub directory: String,
    #[serde(with = "date_format")]
    pub date: NaiveDateTime,
    pub chapter_name: Option<String>,
}

mod date_format {
    use chrono::NaiveDateTime;
    use serde::{self, Deserialize, Deserializer};

    const FORMAT: &'static str = "%Y-%m-%d %H:%M:%S";

    pub fn deserialize<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        NaiveDateTime::parse_from_str(&s, FORMAT).map_err(serde::de::Error::custom)
    }
}

pub struct Mangasee {
    url: String,
}

impl Mangasee {
    pub fn new() -> Mangasee {
        Mangasee {
            url: "https://mangasee123.com".to_string(),
        }
    }
}

impl Extension for Mangasee {
    fn detail(&self) -> Source {
        Source {
            id: ID,
            name: NAME.to_string(),
            url: self.url.clone(),
            version: std::env!("PLUGIN_VERSION").to_string(),
            icon: "https://mangasee123.com/media/favicon.png".to_string(),
            need_login: false,
        }
    }

    fn get_manga_list(&self, param: Param) -> Result<Vec<Manga>, Box<dyn Error>> {
        let vm_dir = {
            let resp = http_request(Request {
                method: "GET".to_string(),
                url: format!("{}/search", &self.url),
                headers: None,
            });
            if resp.status > 299 {
                return Err("http request error".into());
            }
            let html = resp.body;
            if let Some(i) = html.find("vm.Directory =") {
                let dir = &html[i + 15..];
                if let Some(i) = dir.find("}];") {
                    let vm_dir = &dir[..i + 2];
                    vm_dir.to_string()
                } else {
                    return Err("error get manga".into());
                }
            } else {
                return Err("list not found".into());
            }
        };

        let mut dirs = serde_json::from_str::<Vec<Dir>>(&vm_dir)?;

        let sort_by = param.sort_by.unwrap_or(SortByParam::Views);
        let sort_order = param.sort_order.unwrap_or(SortOrderParam::Asc);

        if let Some(keyword) = param.keyword {
            dirs.retain(|d| d.s.to_lowercase().contains(&keyword.to_lowercase()))
        }

        match sort_by {
            SortByParam::Views => {
                dirs.sort_by(|a, b| {
                    let v_a = a.v.parse::<i32>().unwrap();
                    let v_b = b.v.parse::<i32>().unwrap();
                    match sort_order {
                        SortOrderParam::Asc => v_a.cmp(&v_b),
                        SortOrderParam::Desc => v_b.cmp(&v_a),
                    }
                });
            }
            SortByParam::Comment => {}
            SortByParam::LastUpdated => {
                dirs.sort_by(|a, b| match sort_order {
                    SortOrderParam::Asc => a.lt.cmp(&b.lt),
                    SortOrderParam::Desc => b.lt.cmp(&a.lt),
                });
            }
            SortByParam::Title => {
                dirs.sort_by(|a, b| match sort_order {
                    SortOrderParam::Asc => a.s.cmp(&b.s),
                    SortOrderParam::Desc => b.s.cmp(&a.s),
                });
            }
        }

        let page = param.page.map(|p| p as usize).unwrap_or(1);
        let offset = (page - 1) * 20;
        if offset >= dirs.len() {
            return Err("no page".into());
        }
        let mangas = match dirs[offset..].len() {
            0..=20 => &dirs[offset..],
            _ => &dirs[offset..offset + 20],
        };

        return Ok(mangas.par_iter().map(|d| d.into()).collect());
    }

    /// Get the rest of details unreachable from `get_mangas`
    fn get_manga_info(&self, path: &String) -> Result<Manga, Box<dyn Error>> {
        let resp = http_request(Request {
            method: "GET".to_string(),
            url: format!("{}{}", &self.url, &path),
            headers: None,
        });
        if resp.status > 299 {
            return Err("http request error".into());
        }
        let html = resp.body;

        let document = scraper::Html::parse_document(&html);

        let title = {
            let mut title = None;
            let selector =
                scraper::Selector::parse("li[class=\"list-group-item d-none d-sm-block\"] h1")
                    .map_err(|e| format!("{:?}", e))?;
            for element in document.select(&selector) {
                for text in element.text() {
                    if !text.is_empty() {
                        title = Some(text.to_string());
                        break;
                    }
                }
            }
            if let Some(title) = title {
                title
            } else {
                return Err("no title".into());
            }
        };

        let description = {
            let mut desc = None;
            let selector = scraper::Selector::parse("div[class=\"top-5 Content\"]")
                .map_err(|e| format!("{:?}", e))?;
            for element in document.select(&selector) {
                for text in element.text() {
                    desc = Some(text.to_string());
                    break;
                }
            }
            desc
        };

        let mut author = vec![];
        let selector = scraper::Selector::parse("a[href^=\"/search/?author=\"]")
            .map_err(|e| format!("{:?}", e))?;
        for element in document.select(&selector) {
            for text in element.text() {
                author.push(text.to_string());
            }
        }

        let mut genre = vec![];
        let selector = scraper::Selector::parse("a[href^=\"/search/?genre=\"]")
            .map_err(|e| format!("{:?}", e))?;
        for element in document.select(&selector) {
            for text in element.text() {
                genre.push(String::from(text));
            }
        }

        let status = {
            let mut status = None;
            let selector = scraper::Selector::parse("a[href^=\"/search/?status=\"]")
                .map_err(|e| format!("{:?}", e))?;
            for element in document.select(&selector) {
                status = element.value().attr("href").map(|h| {
                    h.strip_prefix("/search/?status=")
                        .map(|s| s.to_string())
                        .unwrap_or(h.to_string())
                });
                break;
            }
            status
        };

        let mut cover_url = "".to_string();
        let selector = scraper::Selector::parse("img[class=\"img-fluid bottom-5\"]")
            .map_err(|e| format!("{:?}", e))?;
        for element in document.select(&selector) {
            cover_url = element
                .value()
                .attr("src")
                .map(|src| src.to_string())
                .ok_or(format!("no src"))?;
            break;
        }

        Ok(Manga {
            source_id: ID,
            title,
            description,
            author,
            genre,
            status,
            path: path.clone(),
            cover_url,
        })
    }

    fn get_chapters(&self, path: &String) -> Result<Vec<Chapter>, Box<dyn Error>> {
        let resp = http_request(Request {
            method: "GET".to_string(),
            url: format!("{}{}", &self.url, &path),
            headers: None,
        });
        if resp.status > 299 {
            return Err("http request error".into());
        }
        let html = resp.body;

        let index_name = {
            let mat = Regex::new(r#"(?<=vm\.IndexName = ").*(?=";)"#)
                .unwrap()
                .find(&html)
                .unwrap();
            mat.unwrap().as_str().to_string()
        };

        let vm_dir = {
            let mat = Regex::new(r#"(?<=vm\.Chapters = )\[.*\](?=;)"#)
                .unwrap()
                .find(&html)
                .unwrap();
            mat.unwrap().as_str().to_string()
        };

        let ch_dirs: Vec<DirChapter> = match serde_json::from_str::<Vec<DirChapter>>(&vm_dir) {
            Ok(dirs) => dirs
                .par_iter()
                .map(|d| DirChapter {
                    index_name: index_name.to_string(),
                    ..d.clone()
                })
                .collect(),
            Err(e) => return Err(e.into()),
        };

        let mut chapters = vec![];
        for ch in ch_dirs.iter() {
            let mut chapter = ch.chapter.clone();
            let t = chapter.remove(0);

            let index = if t != '1' {
                format!("-index-{}", t)
            } else {
                "".to_string()
            };

            chapter.insert_str(chapter.len() - 1, ".");
            let number = chapter.parse::<f64>()?;

            chapters.push(Chapter {
                source_id: ID,
                title: format!("{} {}", ch.type_field, number.to_string()),
                path: format!(
                    "/read-online/{}-chapter-{}{}.html",
                    &index_name,
                    number.to_string(),
                    index,
                ),
                uploaded: ch.date,
                number: number + if index == "" { 0.0 } else { 10000.0 },
                scanlator: "".to_string(),
            })
        }

        Ok(chapters)
    }

    fn get_pages(&self, path: &String) -> Result<Vec<String>, Box<dyn Error>> {
        let resp = http_request(Request {
            method: "GET".to_string(),
            url: format!("{}{}", &self.url, &path),
            headers: None,
        });
        if resp.status > 299 {
            return Err("http request error".into());
        }
        let html = resp.body;

        let index_name = {
            let mat = Regex::new(r#"(?<=vm\.IndexName = ").*(?=";)"#)
                .unwrap()
                .find(&html)
                .unwrap();
            mat.unwrap().as_str().to_string()
        };

        let cur_chapter = {
            let mat = Regex::new(r"(?<=vm\.CurChapter = ){.*}(?=;)")
                .unwrap()
                .find(&html)
                .unwrap();
            let cur_chapter_str = mat.unwrap().as_str();
            serde_json::from_str::<CurChapter>(cur_chapter_str).unwrap()
        };

        let cur_path_name = {
            let mat = Regex::new(r#"(?<=vm\.CurPathName = ").*(?=";)"#)
                .unwrap()
                .find(&html)
                .unwrap();
            mat.unwrap().as_str().to_string()
        };

        // https://{{vm.CurPathName}}/manga/Sono-Bisque-Doll-Wa-Koi-Wo-Suru/{{vm.CurChapter.Directory == '' ? '' : vm.CurChapter.Directory+'/'}}{{vm.ChapterImage(vm.CurChapter.Chapter)}}-{{vm.PageImage(Page)}}.png
        let directory = {
            if cur_chapter.directory == "" {
                "".to_string()
            } else {
                format!("{}/", cur_chapter.directory)
            }
        };
        let chapter_image = {
            /*
            vm.ChapterImage = function(ChapterString){
                var Chapter = ChapterString.slice(1,-1);
                var Odd = ChapterString[ChapterString.length -1];
                if(Odd == 0){
                    return Chapter;
                }else{
                    return Chapter + "." + Odd;
                }
            };
            */
            let chapter = cur_chapter.chapter[1..cur_chapter.chapter.len() - 1].to_string();
            let odd = cur_chapter.chapter[cur_chapter.chapter.len() - 1..].to_string();
            if odd == "0" {
                chapter
            } else {
                format!("{}.{}", chapter, odd)
            }
        };

        let page = cur_chapter.page.parse::<i32>().unwrap_or(0);
        let mut pages = Vec::new();
        for i in 1..page + 1 {
            let page_image = {
                /*
                vm.PageImage = function(PageString){
                    var s = "000" + PageString;
                    return s.substr(s.length - 3);
                }
                */
                let s = format!("000{}", i);
                s[(s.len() - 3)..].to_string()
            };

            pages.push(format!(
                "https://{}/manga/{}/{}{}-{}.png",
                cur_path_name, index_name, directory, chapter_image, page_image
            ));
        }

        Ok(pages)
    }
}
