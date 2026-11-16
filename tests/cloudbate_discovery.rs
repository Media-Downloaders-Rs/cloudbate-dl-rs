use personal_downloaders::{
    Cancel, Site,
    sites::cloudbate,
    state::{State, representation_key, source_key},
};
use std::{cell::RefCell, future::ready};
use url::Url;
const PROFILE: &str = "https://www.cloudbate.com/models/model-one/";
fn post(id: u64) -> String {
    format!("https://www.cloudbate.com/video/{id}/sample/")
}
fn media(id: u64, quality: &str, token: &str) -> String {
    let suffix = if quality == "source" {
        String::new()
    } else {
        format!("_{quality}")
    };
    format!(
        "https://www.cloudbate.com/get_file/6/{}/{}//",
        "a".repeat(32),
        id
    )
    .replace(
        &format!("/{id}//"),
        &format!(
            "/{}/{id}/{id}{suffix}.mp4/?v-acctoken={token}",
            id / 1000 * 1000
        ),
    )
}
fn video(id: u64) -> String {
    format!(
        "<script>var cfg = {{\n video_id: '{id}',\n video_models: 'model_one',\n event_reporting2: '{}',\n video_url: '{}',\n preview_url: 'https://www.cloudbate.com/contents/preview.jpg'\n}}; window['player_obj']=kt_player('kt_player','player.swf','100%','100%',cfg);</script>",
        media(id, "source", "analytics"),
        media(id, "720p", "tokenA")
    )
}
fn profile(ids: &[u64], next: bool) -> String {
    let links = ids
        .iter()
        .map(|id| format!("<a href='{}'>video</a>", post(*id)))
        .collect::<String>();
    format!(
        "<link rel='canonical' href='{PROFILE}'><div id='list_videos_common_videos_list'><div id='list_videos_common_videos_list_items'>{links}</div>{}</div><div id='list_videos_popular_videos_list'><a href='{}'>unrelated</a></div>",
        if next {
            format!("<div class='pagination'><a href='{PROFILE}2/'>Next</a></div>")
        } else {
            String::new()
        },
        post(9999)
    )
}
#[test]
fn strict_profile_native_video_and_signed_media_policies() {
    assert_eq!(
        cloudbate::normalize("https://cloudbate.com/models/model-one/").unwrap(),
        PROFILE
    );
    for u in [
        "http://www.cloudbate.com/models/model-one/",
        "https://www.cloudbate.com.evil.test/models/model-one/",
        "https://u:p@www.cloudbate.com/models/model-one/",
        "https://www.cloudbate.com/models/model-one/?sort=views",
    ] {
        assert!(cloudbate::normalize(u).is_err());
    }
    let u = Url::parse(&media(1234, "720p", "tokenA")).unwrap();
    assert!(cloudbate::validate_request(&u, &post(1234)).is_ok());
    assert!(cloudbate::validate_request(&u, &post(1235)).is_err());
    let cdn = Url::parse("https://nubisera.top/remote_control.php?file=opaque.mp4&acctoken=token")
        .unwrap();
    assert!(cloudbate::validate_redirect(&cdn, &u, &post(1234)).is_ok());
    assert!(
        cloudbate::validate_redirect(
            &Url::parse("https://evil.test/remote_control.php?file=opaque.mp4&acctoken=token")
                .unwrap(),
            &u,
            &post(1234)
        )
        .is_err()
    );
    assert!(
        cloudbate::validate_media(
            &Url::parse(
                "https://nubisera.top/remote_control.php?file=https://localhost/x&acctoken=t"
            )
            .unwrap(),
            &post(1234)
        )
        .is_err()
    );
}
#[test]
fn owned_profile_list_excludes_recommendations_and_bounds_pagination() {
    let p = cloudbate::profile_page(&profile(&[1234, 1235], true), PROFILE, PROFILE).unwrap();
    assert_eq!(p.videos.len(), 2);
    assert_eq!(
        p.next.as_deref(),
        Some("https://www.cloudbate.com/models/model-one/2/")
    );
    assert!(
        cloudbate::profile_page(
            &profile(&[1234], false).replace(
                "rel='canonical' href='https://www.cloudbate.com/models/model-one/'",
                "rel='canonical' href='https://www.cloudbate.com/models/foreign/'"
            ),
            PROFILE,
            PROFILE
        )
        .is_err()
    );
}
#[test]
fn only_bound_player_sources_and_matching_model_ids_are_accepted() {
    let file = cloudbate::parse_video(&video(1234), PROFILE, &post(1234)).unwrap();
    assert!(file.url.contains("_720p.mp4"));
    assert!(!file.url.contains("analytics"));
    assert!(file.fallback.is_none());
    assert!(file.sha256.is_empty());
    assert!(file.path.ends_with("1234_720p.mp4"));
    assert!(
        cloudbate::parse_video(
            &video(1234).replace("model_one", "foreign"),
            PROFILE,
            &post(1234)
        )
        .is_err()
    );
    assert!(cloudbate::parse_video(&video(1234), PROFILE, &post(1235)).is_err());
    assert!(
        cloudbate::parse_video(
            &video(1234).replace("video_url:", "advert_url:"),
            PROFILE,
            &post(1234)
        )
        .is_err()
    );
}
#[test]
fn signatures_do_not_change_completion_or_compatible_partial_identity() {
    let mut a = cloudbate::parse_video(&video(1234), PROFILE, &post(1234)).unwrap();
    a.expected_size = Some(100);
    let mut b = a.clone();
    b.url = media(1234, "720p", "tokenB");
    assert_eq!(source_key(&a).unwrap(), source_key(&b).unwrap());
    assert_eq!(
        representation_key(&a).unwrap(),
        representation_key(&b).unwrap()
    );
    b.expected_size = Some(200);
    assert_ne!(
        representation_key(&a).unwrap(),
        representation_key(&b).unwrap()
    );
    assert!(!source_key(&a).unwrap().contains("tokenA"));
}
#[tokio::test]
async fn complete_catalog_cache_update_and_pending_retry_keep_valid_files() {
    let root = tempfile::tempdir().unwrap();
    let state = State::open(root.path(), Site::Cloudbate).unwrap();
    let fetch = |u: String| {
        ready(Ok(if u == PROFILE {
            profile(&[1234], false)
        } else {
            video(1234)
        }))
    };
    let old = cloudbate::resolve(&state, PROFILE, false, false, &Cancel::default(), &fetch)
        .await
        .unwrap();
    assert_eq!(old.files.len(), 1);
    assert_eq!(
        cloudbate::resolve(&state, PROFILE, false, false, &Cancel::default(), &|_| {
            ready(Err(anyhow::anyhow!("cache only")))
        })
        .await
        .unwrap()
        .files
        .len(),
        1
    );
    let calls = RefCell::new(Vec::new());
    let changed = cloudbate::resolve(
        &state,
        PROFILE,
        false,
        true,
        &Cancel::default(),
        &|u: String| {
            calls.borrow_mut().push(u.clone());
            ready(if u == PROFILE {
                Ok(profile(&[1235, 1234], false))
            } else {
                Err(anyhow::anyhow!("fixture unavailable"))
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(changed.files.len(), 1);
    assert_eq!(changed.pending.len(), 1);
    assert_eq!(calls.borrow().len(), 2);
    let retried = cloudbate::resolve(
        &state,
        PROFILE,
        false,
        false,
        &Cancel::default(),
        &|u: String| {
            ready(Ok(if u == PROFILE {
                profile(&[1235, 1234], false)
            } else {
                video(1235)
            }))
        },
    )
    .await
    .unwrap();
    assert_eq!(retried.files.len(), 2);
    assert!(retried.pending.is_empty());
    let before = state.post_snapshot(PROFILE).unwrap();
    assert!(
        cloudbate::resolve(
            &state,
            PROFILE,
            true,
            false,
            &Cancel::default(),
            &|_| ready(Err(anyhow::anyhow!("failed refresh")))
        )
        .await
        .is_err()
    );
    assert_eq!(state.post_snapshot(PROFILE).unwrap(), before);
}
#[tokio::test]
async fn fresh_direct_source_is_head_verified_and_remembered_for_cached_listings() {
    let root = tempfile::tempdir().unwrap();
    let state = State::open(root.path(), Site::Cloudbate).unwrap();
    let listing = cloudbate::resolve(
        &state,
        PROFILE,
        false,
        false,
        &Cancel::default(),
        &|u: String| {
            ready(Ok(if u == PROFILE {
                profile(&[1234], false)
            } else {
                video(1234)
            }))
        },
    )
    .await
    .unwrap();
    let ready = cloudbate::prepare(
        &state,
        &listing.files[0],
        &Cancel::default(),
        &|_| std::future::ready(Ok(video(1234))),
        &|url: String, context: String| {
            assert!(url.contains("get_file"));
            assert_eq!(context, post(1234));
            std::future::ready(Ok(cloudbate::HeadInfo {
                url: "https://nubisera.top/remote_control.php?file=opaque.mp4&acctoken=fresh"
                    .into(),
                size: 100,
                mime: "video/mp4".into(),
            }))
        },
    )
    .await
    .unwrap();
    assert_eq!(ready.expected_size, Some(100));
    assert!(ready.url.contains("nubisera.top"));
    let cached = cloudbate::resolve(&state, PROFILE, false, false, &Cancel::default(), &|_| {
        std::future::ready(Err(anyhow::anyhow!("cache")))
    })
    .await
    .unwrap();
    assert_eq!(cached.files[0].url, ready.url);
}

#[tokio::test]
async fn completed_cloudbate_video_is_skipped_without_renewing_its_expired_url() {
    use personal_downloaders::engine::{Action, Engine, Event, RunOptions};
    let root = tempfile::tempdir().unwrap();
    let state = State::open(root.path(), Site::Cloudbate).unwrap();
    let listing = cloudbate::resolve(
        &state,
        PROFILE,
        false,
        false,
        &Cancel::default(),
        &|u: String| {
            ready(Ok(if u == PROFILE {
                profile(&[1234], false)
            } else {
                video(1234)
            }))
        },
    )
    .await
    .unwrap();
    let file = &listing.files[0];
    let target = state.target(&file.path).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, b"test").unwrap();
    let hash = personal_downloaders::state::hash_file(&target, &Cancel::default()).unwrap();
    state
        .record(
            file,
            4,
            &hash,
            personal_downloaders::mtime(&target).unwrap(),
            "complete",
        )
        .unwrap();
    drop(state);
    let engine = Engine::new(
        RunOptions {
            site: Site::Cloudbate,
            urls: vec![PROFILE.into()],
            root: root.path().into(),
            action: Action::Download,
            workers: 4,
            password: None,
        },
        Cancel::default(),
        std::sync::Arc::new(|e| {
            if let Event::Message(s) = e {
                assert!(
                    !s.starts_with("Leyendo ") && !s.starts_with("Renovando"),
                    "completed file must not reach network"
                );
            }
        }),
    )
    .unwrap();
    let result = engine.run().await.unwrap();
    assert_eq!(result.skipped, 1);
    assert_eq!(result.failed, 0);
}

#[test]
fn minified_player_config_keeps_quoted_commas_and_fake_field_names_inside_titles() {
    let compact = video(1234).replace('\n', " ");
    let file = cloudbate::parse_video(&compact, PROFILE, &post(1234)).unwrap();
    assert!(file.url.contains("_720p.mp4"));
    let tricky = compact.replace(
        "video_models:",
        "video_title: \"quoted, video_url: 'https://evil.test/fake.mp4'\", video_models:",
    );
    let file = cloudbate::parse_video(&tricky, PROFILE, &post(1234)).unwrap();
    assert!(file.url.contains("get_file/6/"));
}

#[tokio::test]
async fn a_cached_post_that_failed_refresh_stays_pending_until_a_real_retry_succeeds() {
    let root = tempfile::tempdir().unwrap();
    let state = State::open(root.path(), Site::Cloudbate).unwrap();
    cloudbate::resolve(
        &state,
        PROFILE,
        false,
        false,
        &Cancel::default(),
        &|u: String| {
            ready(Ok(if u == PROFILE {
                profile(&[1234], false)
            } else {
                video(1234)
            }))
        },
    )
    .await
    .unwrap();
    let failed = cloudbate::resolve(
        &state,
        PROFILE,
        true,
        false,
        &Cancel::default(),
        &|u: String| {
            ready(if u == PROFILE {
                Ok(profile(&[1234], false))
            } else {
                Err(anyhow::anyhow!("metadata unavailable"))
            })
        },
    )
    .await
    .unwrap();
    assert_eq!(failed.files.len(), 1);
    assert_eq!(failed.pending.len(), 1);
    let retried = RefCell::new(0);
    let failed = cloudbate::resolve(&state, PROFILE, false, false, &Cancel::default(), &|_| {
        *retried.borrow_mut() += 1;
        ready(Err(anyhow::anyhow!("still unavailable")))
    })
    .await
    .unwrap();
    assert_eq!(*retried.borrow(), 1);
    assert_eq!(failed.pending.len(), 1);
    let recovered = cloudbate::resolve(
        &state,
        PROFILE,
        false,
        true,
        &Cancel::default(),
        &|u: String| {
            ready(Ok(if u == PROFILE {
                profile(&[1234], false)
            } else {
                video(1234)
            }))
        },
    )
    .await
    .unwrap();
    assert_eq!(recovered.files.len(), 1);
    assert!(recovered.pending.is_empty());
}
#[test]
fn active_ajax_pagination_is_not_silently_truncated() {
    let html=profile(&[1234],false).replace("</div><div id='list_videos_popular_videos_list'>","<div class='pagination'><a href='#' data-action='ajax' data-parameters='from_videos:2'>Next</a></div></div><div id='list_videos_popular_videos_list'>");
    assert!(cloudbate::profile_page(&html, PROFILE, PROFILE).is_err());
    let disabled = html.replace(
        "href='#' data-action",
        "href='#' aria-disabled='true' data-action",
    );
    assert!(
        cloudbate::profile_page(&disabled, PROFILE, PROFILE)
            .unwrap()
            .next
            .is_none()
    );
}

#[test]
fn observed_naturgut_redirect_is_accepted_without_relaxing_other_hosts() {
    let native = Url::parse(&media(882654, "720p", "public-token")).unwrap();
    let target =
        Url::parse("https://naturgut.top/remote_control.php?file=opaque.mp4&acctoken=token")
            .unwrap();
    cloudbate::validate_redirect(&target, &native, &post(882654)).unwrap();
    for bad in [
        "https://naturgut.top.evil.test/remote_control.php?file=opaque.mp4&acctoken=token",
        "https://evilnaturgut.top/remote_control.php?file=opaque.mp4&acctoken=token",
        "http://naturgut.top/remote_control.php?file=opaque.mp4&acctoken=token",
        "https://naturgut.top/other.php?file=opaque.mp4&acctoken=token",
        "https://naturgut.top/remote_control.php?file=https://localhost/video.mp4&acctoken=token",
    ] {
        assert!(
            cloudbate::validate_redirect(&Url::parse(bad).unwrap(), &native, &post(882654))
                .is_err()
        );
    }
}
