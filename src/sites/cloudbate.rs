//! Public model HTML and declared KVS MP4 links. Never use previews/analytics.
use crate::{Cancel, File, Listing, Pending, safe_name, state::State};
use anyhow::{Context, Result, bail};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::LazyLock,
};
use url::Url;

const HOST: &str = "www.cloudbate.com";
const CDNS: &[&str] = &[
    "nubisera.top",
    "naturgut.top",
    "gordafen.top",
    "bolvesta.top",
    "etarefde.top",
    "topgordafen.top",
    "k4rsr.cdnvideocb.com",
];
static SLUG: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"^[a-z0-9]+(?:-[a-z0-9]+)*$").unwrap());
static BINDING: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"(?s)kt_player\(\s*['"]kt_player['"].*?,\s*([A-Za-z_$][A-Za-z0-9_$]*)\s*\)"#,
    )
    .unwrap()
});
fn secure(u: &Url) -> bool {
    u.scheme() == "https"
        && u.port().is_none()
        && u.username().is_empty()
        && u.password().is_none()
        && u.fragment().is_none()
}
fn site(u: &Url) -> bool {
    secure(u) && matches!(u.host_str(), Some("cloudbate.com" | "www.cloudbate.com"))
}
fn model_page(u: &Url) -> Result<(String, usize)> {
    if !site(u) {
        bail!("Introduce un perfil HTTPS de Cloudbate")
    }
    let p = u.path().trim_matches('/').split('/').collect::<Vec<_>>();
    if !(p.len() == 2 || p.len() == 3)
        || p[0] != "models"
        || p[1].len() > 100
        || !SLUG.is_match(p[1])
    {
        bail!("Perfil de Cloudbate no válido")
    }
    let mut number = if p.len() == 3 {
        p[2].parse::<usize>()?
    } else {
        1
    };
    let q = u.query_pairs().collect::<Vec<_>>();
    if !q.is_empty() {
        if p.len() != 2 || q.len() != 1 || !matches!(q[0].0.as_ref(), "page" | "from") {
            bail!("Paginación no admitida")
        }
        number = q[0].1.parse()?;
    }
    if !(1..=1000).contains(&number) {
        bail!("Paginación fuera de límite")
    }
    Ok((p[1].into(), number))
}
pub fn normalize(input: &str) -> Result<String> {
    let u = Url::parse(input)?;
    let (owner, page) = model_page(&u)?;
    if page != 1 || u.query().is_some() || u.path().trim_matches('/').split('/').count() != 2 {
        bail!("Introduce el perfil sin filtros ni paginación")
    }
    Ok(format!("https://{HOST}/models/{owner}/"))
}
fn video_url(input: &str) -> Result<(String, String)> {
    let u = Url::parse(input)?;
    let p = u.path().trim_matches('/').split('/').collect::<Vec<_>>();
    if !site(&u)
        || u.query().is_some()
        || p.len() != 3
        || p[0] != "video"
        || p[1].is_empty()
        || p[1].len() > 12
        || !p[1].bytes().all(|b| b.is_ascii_digit())
        || p[1].parse::<u64>()? == 0
        || p[2].len() > 250
        || !SLUG.is_match(p[2])
    {
        bail!("Página de vídeo no válida")
    }
    Ok((
        p[1].into(),
        format!("https://{HOST}/video/{}/{}/", p[1], p[2]),
    ))
}
pub fn is_video(input: &str) -> bool {
    video_url(input).is_ok()
}
pub fn is_context(input: &str) -> bool {
    Url::parse(input).is_ok_and(|u| model_page(&u).is_ok()) || is_video(input)
}
fn quality(value: &str) -> bool {
    value == "source"
        || value
            .strip_suffix('p')
            .is_some_and(|n| n.parse::<u32>().is_ok_and(|v| (1..=8640).contains(&v)))
}
fn signed(u: &Url) -> Result<(String, String)> {
    let p = u.path().trim_matches('/').split('/').collect::<Vec<_>>();
    if !site(u)
        || p.len() != 6
        || p[0] != "get_file"
        || p[1].parse::<u32>().is_err()
        || p[2].len() != 32
        || !p[2].bytes().all(|b| b.is_ascii_hexdigit())
        || p[4].len() > 12
        || !p[4].bytes().all(|b| b.is_ascii_digit())
    {
        bail!("Enlace firmado no válido")
    }
    let id = p[4].parse::<u64>()?;
    if id == 0 || p[3].parse::<u64>()? != id / 1000 * 1000 {
        bail!("El enlace firmado cambia de vídeo")
    }
    let stem = p[5]
        .strip_suffix(".mp4")
        .context("La fuente declarada no es MP4")?;
    let q = if stem == p[4] {
        "source".to_owned()
    } else {
        stem.strip_prefix(&format!("{}_", p[4]))
            .filter(|s| quality(s))
            .context("Calidad de vídeo no válida")?
            .into()
    };
    let pairs = u.query_pairs().collect::<Vec<_>>();
    if pairs.len() != 1 || pairs[0].0 != "v-acctoken" || !token(&pairs[0].1) {
        bail!("Firma pública ausente o no válida")
    }
    Ok((p[4].into(), q))
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 8192
        && s.bytes()
            .all(|b| b.is_ascii_graphic() && !matches!(b, b'<' | b'>' | b'"' | b'\''))
}
fn cdn(u: &Url) -> Result<String> {
    if !secure(u)
        || !u.host_str().is_some_and(|h| CDNS.contains(&h))
        || u.path() != "/remote_control.php"
    {
        bail!("Servidor de Cloudbate no admitido")
    }
    let pairs = u.query_pairs().collect::<Vec<_>>();
    let q = pairs.iter().cloned().collect::<HashMap<_, _>>();
    let file = q.get("file").context("Falta el objeto de vídeo")?;
    if pairs.len() != 2
        || q.len() != 2
        || !q.get("acctoken").is_some_and(|v| token(v))
        || !file.ends_with(".mp4")
        || file.len() > 8192
        || !file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'='))
    {
        bail!("Objeto firmado de CDN no válido")
    }
    Ok(file.to_string())
}
pub fn validate_media(u: &Url, post: &str) -> Result<()> {
    let (id, _) = video_url(post)?;
    if site(u) {
        if signed(u)?.0 != id {
            bail!("El enlace pertenece a otro vídeo")
        }
    } else {
        cdn(u)?;
    }
    Ok(())
}
pub fn validate_request(u: &Url, post: &str) -> Result<()> {
    if let Ok(parsed) = Url::parse(post)
        && let Ok((owner, _)) = model_page(&parsed)
    {
        if model_page(u)?.0 != owner {
            bail!("La página pertenece a otro perfil")
        }
        return Ok(());
    }
    let (id, _) = video_url(post)?;
    if let Ok((other, _)) = video_url(u.as_str()) {
        if other == id {
            return Ok(());
        }
        bail!("La página pertenece a otro vídeo")
    }
    validate_media(u, post)
}
pub fn validate_redirect(u: &Url, original: &Url, post: &str) -> Result<()> {
    validate_request(original, post)?;
    if let Ok((owner, page)) = model_page(original) {
        if model_page(u)? != (owner, page) {
            bail!("La redirección cambia el perfil o la página")
        }
    } else if let Ok((id, _)) = video_url(original.as_str()) {
        if video_url(u.as_str())?.0 != id {
            bail!("La redirección cambia el vídeo")
        }
    } else {
        validate_media(u, post)?;
        if site(original) && site(u) && signed(original)? != signed(u)? {
            bail!("La redirección cambia la calidad")
        }
        if !site(original) && cdn(original)? != cdn(u)? {
            bail!("La redirección cambia el objeto de CDN")
        }
    }
    Ok(())
}
fn parts(file: &File) -> Result<(String, String, String)> {
    let p = file.id.split(':').collect::<Vec<_>>();
    if p.len() != 3
        || p[0].len() > 100
        || !SLUG.is_match(p[0])
        || !quality(p[2])
        || video_url(&file.post)?.0 != p[1]
    {
        bail!("Identidad de vídeo no válida")
    }
    validate_media(&Url::parse(&file.url)?, &file.post)?;
    if site(&Url::parse(&file.url)?)
        && signed(&Url::parse(&file.url)?)? != (p[1].into(), p[2].into())
    {
        bail!("La fuente cambia de calidad")
    }
    if file
        .expected_size
        .is_some_and(|n| n == 0 || n > i64::MAX as u64)
        || !file.sha256.is_empty()
        || !file.md5.is_empty()
        || file.fallback.is_some()
    {
        bail!("Metadatos de vídeo no válidos")
    }
    Ok((p[0].into(), p[1].into(), p[2].into()))
}
pub fn source_identity(file: &File) -> Result<String> {
    let (owner, id, q) = parts(file)?;
    Ok(format!("cloudbate:{owner}:{id}:{q}"))
}
fn folder(owner: &str) -> String {
    let digest = format!("{:x}", Sha256::digest(owner.as_bytes()));
    format!(
        "{} [cloudbate-{}]",
        safe_name(owner).chars().take(32).collect::<String>(),
        &digest[..32]
    )
}
fn valid_file(file: &File, owner: &str) -> Result<()> {
    let (o, id, q) = parts(file)?;
    if o != owner || file.path != format!("{}/{}_{}.mp4", folder(owner), id, q) {
        bail!("El destino no corresponde al perfil")
    };
    Ok(())
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    pub url: String,
}
pub struct Page {
    pub videos: Vec<Video>,
    pub next: Option<String>,
}
pub fn profile_page(html: &str, input: &str, page: &str) -> Result<Page> {
    let input = normalize(input)?;
    let owner = model_page(&Url::parse(&input)?)?.0;
    let (page_owner, number) = model_page(&Url::parse(page)?)?;
    if page_owner != owner {
        bail!("La pagina cambia de perfil")
    }
    let doc = Html::parse_document(html);
    let canonical = doc
        .select(&Selector::parse("link[rel='canonical']").unwrap())
        .next()
        .and_then(|e| e.value().attr("href"))
        .context("La respuesta no identifica el perfil")?;
    if model_page(&Url::parse(canonical)?)?.0 != owner {
        bail!("La respuesta identifica otro perfil")
    }
    let list = doc
        .select(&Selector::parse("#list_videos_common_videos_list_items").unwrap())
        .next()
        .context("No se encontró la lista del perfil")?;
    let base = Url::parse(&input)?;
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    for link in list.select(&Selector::parse("a[href]").unwrap()) {
        let u = base.join(link.value().attr("href").unwrap())?;
        if !u.path().starts_with("/video/") {
            continue;
        }
        let (id, url) = video_url(u.as_str())?;
        if seen.insert(id.clone()) {
            videos.push(Video { id, url });
        }
    }
    let block = doc
        .select(&Selector::parse("#list_videos_common_videos_list").unwrap())
        .next()
        .context("Falta el bloque del perfil")?;
    let mut next = None;
    for link in block.select(&Selector::parse(".pagination a[href], a[rel='next']").unwrap()) {
        let href = link.value().attr("href").unwrap();
        let inactive = |element: scraper::ElementRef<'_>| {
            element.value().attr("aria-disabled") == Some("true")
                || element
                    .value()
                    .classes()
                    .any(|c| matches!(c, "disabled" | "active" | "current"))
        };
        if inactive(link)
            || link
                .parent()
                .and_then(scraper::ElementRef::wrap)
                .is_some_and(inactive)
        {
            continue;
        }
        if href.is_empty() || href.starts_with('#') {
            bail!("La paginación activa requiere un formato AJAX todavía no admitido")
        }
        let u = base.join(href)?;
        let (o, n) = model_page(&u)?;
        if o != owner {
            bail!("La paginación cambia de perfil")
        };
        if n > number && next.as_ref().is_none_or(|(before, _)| n < *before) {
            next = Some((n, u.to_string()));
        }
    }
    if next.as_ref().is_some_and(|(n, _)| *n != number + 1) {
        bail!("La paginación omite páginas")
    }
    Ok(Page {
        videos,
        next: next.map(|(_, u)| u),
    })
}
fn literal(value: &str) -> Result<String> {
    let mut chars = value.trim().chars();
    let quote = chars.next().context("Valor vacío")?;
    if !matches!(quote, '\'' | '"') {
        bail!("Se requiere una fuente pública constante")
    }
    let mut out = String::new();
    while let Some(c) = chars.next() {
        if c == quote {
            if !chars
                .as_str()
                .trim()
                .trim_end_matches(',')
                .trim()
                .is_empty()
            {
                bail!("Valor de reproductor no constante")
            };
            return Ok(out.replace("&amp;", "&"));
        }
        if c == '\\' {
            match chars.next().context("Escape incompleto")? {
                '/' => out.push('/'),
                '\\' => out.push('\\'),
                '\'' => out.push('\''),
                '"' => out.push('"'),
                'u' => {
                    let code = chars.by_ref().take(4).collect::<String>();
                    out.push(
                        char::from_u32(u32::from_str_radix(&code, 16)?)
                            .context("Escape Unicode no válido")?,
                    );
                }
                _ => bail!("Escape de reproductor no admitido"),
            }
        } else {
            out.push(c);
        }
    }
    bail!("Literal de reproductor incompleto")
}
fn fields(body: &str) -> Result<HashMap<String, String>> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut quoted = None;
    let mut escaped = false;
    let mut depth = 0usize;
    for (index, c) in body.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if let Some(quote) = quoted {
            if c == '\\' {
                escaped = true;
            } else if c == quote {
                quoted = None;
            }
            continue;
        }
        match c {
            '\'' | '"' => quoted = Some(c),
            '(' | '[' | '{' => {
                depth += 1;
                if depth > 16 {
                    bail!("Configuración demasiado anidada")
                }
            }
            ')' | ']' | '}' => {
                depth = depth
                    .checked_sub(1)
                    .context("Configuración no equilibrada")?;
            }
            ',' if depth == 0 => {
                pieces.push(&body[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    if quoted.is_some() || escaped || depth != 0 {
        bail!("Configuración incompleta")
    }
    pieces.push(&body[start..]);
    let mut result = HashMap::new();
    for piece in pieces {
        let Some((key, value)) = piece.trim().split_once(':') else {
            continue;
        };
        let key = key.trim();
        if !(key == "video_id"
            || key == "video_models"
            || key == "video_url"
            || key.starts_with("video_alt_url"))
        {
            continue;
        }
        if result.insert(key.to_owned(), literal(value)?).is_some() {
            bail!("Fuente de reproductor duplicada")
        }
    }
    Ok(result)
}
pub fn parse_video(html: &str, input: &str, post: &str) -> Result<File> {
    let owner = model_page(&Url::parse(&normalize(input)?)?)?.0;
    let (id, post) = video_url(post)?;
    let doc = Html::parse_document(html);
    let mut config = None;
    for script in doc.select(&Selector::parse("script").unwrap()) {
        let text = script.inner_html();
        let Some(binding) = BINDING.captures(&text) else {
            continue;
        };
        let re = regex::Regex::new(&format!(
            r"(?s)\bvar\s+{}\s*=\s*\{{(.*?)\}}\s*;",
            regex::escape(&binding[1])
        ))?;
        let body = re
            .captures(&text)
            .context("No se encontró la configuración del reproductor")?;
        let fields = fields(&body[1])?;
        if config.replace(fields).is_some() {
            bail!("Reproductor ambiguo")
        }
    }
    let fields = config.context("No se encontró un reproductor público admitido")?;
    if fields.get("video_id") != Some(&id)
        || !fields.get("video_models").is_some_and(|m| {
            m.split(',')
                .any(|v| v.trim().to_ascii_lowercase().replace('_', "-") == owner)
        })
    {
        bail!("El vídeo no pertenece al perfil solicitado")
    }
    let mut choices = Vec::new();
    for (key, value) in &fields {
        if key == "video_url"
            || key
                .strip_prefix("video_alt_url")
                .is_some_and(|s| s.bytes().all(|b| b.is_ascii_digit()))
        {
            let u = Url::parse(value)?;
            let (other, q) = signed(&u)?;
            if other != id {
                bail!("La fuente pertenece a otro vídeo")
            };
            let rank = q
                .strip_suffix('p')
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0);
            choices.push((rank, q, u.to_string()));
        }
    }
    choices.sort_by_key(|a| std::cmp::Reverse(a.0));
    let (_, q, url) = choices
        .into_iter()
        .next()
        .context("No hay una fuente MP4 declarada")?;
    let file = File {
        id: format!("{owner}:{id}:{q}"),
        post,
        url,
        path: format!("{}/{}_{}.mp4", folder(&owner), id, q),
        ..Default::default()
    };
    valid_file(&file, &owner)?;
    Ok(file)
}
fn post_key(owner: &str, id: &str) -> String {
    format!("cloudbate:post:{owner}:{id}")
}
fn cached_file(state: &State, owner: &str, id: &str) -> Result<Option<File>> {
    let Some(rows) = state.post_snapshot(&post_key(owner, id))? else {
        return Ok(None);
    };
    if rows.len() != 1 {
        bail!("Caché de vídeo incompleta")
    }
    let file: File = serde_json::from_str(&rows[0])?;
    valid_file(&file, owner)?;
    if parts(&file)?.1 != id {
        bail!("Caché de otro vídeo")
    };
    Ok(Some(file))
}
fn remember(state: &State, file: &File) -> Result<()> {
    let (owner, id, _) = parts(file)?;
    valid_file(file, &owner)?;
    state.save_post(&post_key(&owner, &id), &[serde_json::to_string(file)?])
}
#[derive(Serialize, Deserialize)]
struct Snapshot {
    version: u8,
    input: String,
    videos: Vec<Video>,
    listing: Listing,
}
fn valid(snapshot: &Snapshot, input: &str) -> Result<()> {
    let owner = model_page(&Url::parse(input)?)?.0;
    if snapshot.version != 1 || snapshot.input != input || snapshot.listing.title != folder(&owner)
    {
        bail!("Catálogo de perfil no válido")
    }
    let mut ids = HashSet::new();
    for v in &snapshot.videos {
        if video_url(&v.url)?.0 != v.id || !ids.insert(&v.id) {
            bail!("Publicaciones guardadas no válidas")
        }
    }
    let mut files = HashSet::new();
    for f in &snapshot.listing.files {
        valid_file(f, &owner)?;
        let id = parts(f)?.1;
        if !ids.contains(&id) || !files.insert(id) {
            bail!("Archivos guardados repetidos o ajenos")
        }
    }
    Ok(())
}
pub async fn resolve<F, Fut>(
    state: &State,
    input: &str,
    refresh: bool,
    incremental: bool,
    cancel: &Cancel,
    fetch: &F,
) -> Result<Listing>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String>>,
{
    let input = normalize(input)?;
    let owner = model_page(&Url::parse(&input)?)?.0;
    cancel.check()?;
    let old = state
        .post_snapshot(&input)?
        .map(|rows| -> Result<Snapshot> {
            if rows.len() != 1 {
                bail!("Catálogo incompleto")
            };
            let s = serde_json::from_str(&rows[0])?;
            valid(&s, &input)?;
            Ok(s)
        })
        .transpose();
    let old = if refresh { old.unwrap_or(None) } else { old? };
    if !refresh
        && !incremental
        && let Some(s) = &old
        && s.listing.pending.is_empty()
    {
        let mut listing = s.listing.clone();
        for file in &mut listing.files {
            let (_, id, _) = parts(file)?;
            if let Some(fresh) = cached_file(state, &owner, &id)? {
                *file = fresh;
            }
        }
        state.save_listing(&input, &listing)?;
        return Ok(listing);
    }
    let mut videos = Vec::new();
    let mut seen = HashSet::new();
    if !refresh
        && !incremental
        && let Some(s) = &old
    {
        videos = s.videos.clone();
    } else {
        let mut next = Some(input.clone());
        let mut pages = HashSet::new();
        while let Some(page) = next.take() {
            cancel.check()?;
            if !pages.insert(page.clone()) || pages.len() > 1000 {
                bail!("Paginación repetida o demasiado larga")
            };
            let html = fetch(page.clone()).await?;
            cancel.check()?;
            let batch = profile_page(&html, &input, &page)?;
            let mut progress = 0;
            for v in batch.videos {
                if !seen.insert(v.id.clone()) {
                    bail!("Vídeos repetidos entre páginas")
                };
                videos.push(v);
                progress += 1;
            }
            if pages.len() > 1 && progress == 0 {
                bail!("Página de perfil vacía inesperada")
            };
            next = batch.next;
        }
        if incremental
            && !refresh
            && let Some(s) = &old
        {
            for v in &s.videos {
                if seen.insert(v.id.clone()) {
                    videos.push(v.clone());
                }
            }
        }
    }
    let mut listing = Listing {
        title: folder(&owner),
        ..Default::default()
    };
    let mut retry = HashSet::new();
    if !refresh && let Some(previous) = &old {
        for pending in &previous.listing.pending {
            retry.insert(video_url(&pending.url)?.0);
        }
    }
    for v in &videos {
        cancel.check()?;
        let result = async {
            if !refresh
                && !retry.contains(&v.id)
                && let Some(file) = cached_file(state, &owner, &v.id)?
            {
                return Ok(file);
            }
            let html = fetch(v.url.clone()).await?;
            cancel.check()?;
            let file = parse_video(&html, &input, &v.url)?;
            remember(state, &file)?;
            Ok::<_, anyhow::Error>(file)
        }
        .await;
        match result {
            Ok(file) => listing.files.push(file),
            Err(error) => {
                cancel.check()?;
                if let Some(file) = old.as_ref().and_then(|s| {
                    s.listing
                        .files
                        .iter()
                        .find(|f| parts(f).is_ok_and(|(_, id, _)| id == v.id))
                }) {
                    listing.files.push(file.clone());
                }
                listing.pending.push(Pending {
                    url: v.url.clone(),
                    error: error.to_string(),
                });
            }
        }
    }
    let snapshot = Snapshot {
        version: 1,
        input: input.clone(),
        videos,
        listing,
    };
    valid(&snapshot, &input)?;
    cancel.check()?;
    state.save_post(&input, &[serde_json::to_string(&snapshot)?])?;
    state.save_listing(&input, &snapshot.listing)?;
    Ok(snapshot.listing)
}
pub struct HeadInfo {
    pub url: String,
    pub size: u64,
    pub mime: String,
}
pub async fn prepare<F, Fut, H, HFut>(
    state: &State,
    file: &File,
    cancel: &Cancel,
    fetch: &F,
    head: &H,
) -> Result<File>
where
    F: Fn(String) -> Fut,
    Fut: Future<Output = Result<String>>,
    H: Fn(String, String) -> HFut,
    HFut: Future<Output = Result<HeadInfo>>,
{
    let (owner, _, _) = parts(file)?;
    cancel.check()?;
    let profile = format!("https://{HOST}/models/{owner}/");
    let html = fetch(file.post.clone()).await?;
    cancel.check()?;
    let mut fresh = parse_video(&html, &profile, &file.post)?;
    let info = head(fresh.url.clone(), fresh.post.clone()).await?;
    cancel.check()?;
    if info.size == 0
        || info.size > i64::MAX as u64
        || info.mime.split(';').next().unwrap_or("").trim() != "video/mp4"
    {
        bail!("El servidor no identificó un MP4 completo")
    }
    cdn(&Url::parse(&info.url)?)?;
    fresh.url = info.url;
    fresh.expected_size = Some(info.size);
    valid_file(&fresh, &owner)?;
    remember(state, &fresh)?;
    Ok(fresh)
}
