use std::ffi::CString;
use std::path::Path;

use libva::{Display, VAEntrypoint, VAProfile};

pub struct ProbedVaapiNode {
    pub device_path: CString,
    pub matched_profile: VAProfile::Type,
}

fn codec_id_to_va_profiles(codec_id: ffmpeg_sys_next::AVCodecID) -> &'static [VAProfile::Type] {
    use ffmpeg_sys_next::AVCodecID::*;
    match codec_id {
        AV_CODEC_ID_H264 => &[
            VAProfile::VAProfileH264High,
            VAProfile::VAProfileH264Main,
            VAProfile::VAProfileH264ConstrainedBaseline,
        ],
        AV_CODEC_ID_HEVC => &[VAProfile::VAProfileHEVCMain, VAProfile::VAProfileHEVCMain10],
        AV_CODEC_ID_VP9 => &[
            VAProfile::VAProfileVP9Profile0,
            VAProfile::VAProfileVP9Profile2,
        ],
        AV_CODEC_ID_AV1 => &[VAProfile::VAProfileAV1Profile0],
        AV_CODEC_ID_VP8 => &[VAProfile::VAProfileVP8Version0_3],
        _ => &[],
    }
}

fn render_node_candidates() -> Vec<(u32, String)> {
    let mut nodes = Vec::new();
    for idx in 128..=192 {
        let path = format!("/dev/dri/renderD{idx}");
        if !Path::new(&path).exists() {
            continue;
        }
        let vendor_path = format!("/sys/class/drm/renderD{idx}/device/vendor");
        let vendor_raw = std::fs::read_to_string(&vendor_path);
        let priority = match vendor_raw.as_deref().map(str::trim) {
            Ok("0x8086") => 0,
            Ok("0x1002") => 1,
            Ok("0x10de") => 2,
            Ok(_) => 3,
            Err(_) => 4,
        };
        eprintln!(
            "[vaapi-probe] 候補ノード検出 path={path} vendor={:?} priority={priority}",
            vendor_raw.as_deref().map(str::trim)
        );
        nodes.push((priority, path));
    }
    nodes.sort_by_key(|(p, _)| *p);
    eprintln!("[vaapi-probe] 候補ノード総数={}", nodes.len());
    nodes
}

pub fn probe_vaapi_node(
    codec_id: ffmpeg_sys_next::AVCodecID,
    want_10bit: bool,
) -> Option<ProbedVaapiNode> {
    eprintln!("[vaapi-probe] probe_vaapi_node開始 codec_id={codec_id:?} want_10bit={want_10bit}");
    let profiles = codec_id_to_va_profiles(codec_id);
    if profiles.is_empty() {
        eprintln!(
            "[vaapi-probe] codec_id={codec_id:?}に対応するVAProfileマッピング無し、VAAPI探索中断"
        );
        return None;
    }
    eprintln!("[vaapi-probe] 探索対象プロファイル数={}", profiles.len());

    let candidates = render_node_candidates();
    if candidates.is_empty() {
        eprintln!("[vaapi-probe] /dev/dri/renderD* ノード0件、VAAPI探索中断");
        return None;
    }

    for (priority, path) in candidates {
        eprintln!("[vaapi-probe] ノード検証開始 path={path} priority={priority}");
        let display = match Display::open_drm_display(&path) {
            Ok(d) => {
                eprintln!("[vaapi-probe] Display::open_drm_display成功 path={path}");
                d
            }
            Err(e) => {
                eprintln!("[vaapi-probe] Display::open_drm_display失敗 path={path} err={e:?}");
                continue;
            }
        };

        let mut matched_profile: Option<VAProfile::Type> = None;
        for profile in profiles {
            let is_10bit_profile = matches!(
                *profile,
                VAProfile::VAProfileHEVCMain10 | VAProfile::VAProfileVP9Profile2
            );
            if is_10bit_profile != want_10bit {
                eprintln!(
                    "[vaapi-probe] プロファイルビット深度不一致によりスキップ profile={profile:?} is_10bit_profile={is_10bit_profile} want_10bit={want_10bit}"
                );
                continue;
            }
            match display.query_config_entrypoints(*profile) {
                Ok(eps) => {
                    let has_vld = eps.contains(&VAEntrypoint::VAEntrypointVLD);
                    eprintln!(
                        "[vaapi-probe] query_config_entrypoints path={path} profile={profile:?} entrypoints={eps:?} VLD対応={has_vld}"
                    );
                    if has_vld {
                        matched_profile = Some(*profile);
                        break;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "[vaapi-probe] query_config_entrypoints失敗 path={path} profile={profile:?} err={e:?}"
                    );
                }
            }
        }

        drop(display);

        match matched_profile {
            Some(profile) => {
                let verified = crate::vaapi_config_verify::verify_va_config_creatable(
                    &path, profile, want_10bit,
                );
                eprintln!(
                    "[vaapi-probe] vaCreateConfig実検証結果 path={path} profile={profile:?} verified={verified}"
                );
                if !verified {
                    eprintln!(
                        "[vaapi-probe] ノード不採用(vaCreateConfig実検証失敗) path={path}、次候補探索"
                    );
                    continue;
                }
                eprintln!("[vaapi-probe] ノード採用 path={path} profile={profile:?}");
                let device_path = match CString::new(path.clone()) {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("[vaapi-probe] CString変換失敗 path={path} err={e:?}");
                        continue;
                    }
                };
                return Some(ProbedVaapiNode {
                    device_path,
                    matched_profile: profile,
                });
            }
            None => {
                eprintln!(
                    "[vaapi-probe] ノード不採用(該当ビット深度のVLD対応プロファイル無し) path={path}、次候補探索"
                );
            }
        }
    }
    eprintln!("[vaapi-probe] 全ノード探索終了、対応ノード無し");
    None
}
