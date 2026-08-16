use super::AssetFrames;

pub fn render_gallery(assets: &[AssetFrames], manifest_json: &str) -> String {
    let tabs_html = build_tabs(assets);
    let panels_html = build_panels(assets);
    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>distract.nvim Sprite Gallery</title>
{STYLE}
</head>
<body>
<header>
<h1>distract.nvim Sprite Gallery</h1>
<p class="subtitle">Procedurally generated assets &mdash; all states and frames</p>
</header>
<nav id="tabs">{tabs_html}</nav>
<main id="panels">{panels_html}</main>
<script>
const MANIFEST = {manifest_json};
{SCRIPT}
</script>
</body>
</html>
"##,
        STYLE = STYLE_BLOCK,
        SCRIPT = SCRIPT_BLOCK,
        tabs_html = tabs_html,
        panels_html = panels_html,
        manifest_json = manifest_json,
    )
}

fn build_tabs(assets: &[AssetFrames]) -> String {
    assets
        .iter()
        .enumerate()
        .map(|(index, asset)| {
            let active = if index == 0 { " active" } else { "" };
            format!(
                r#"<button class="tab{}" data-asset="{}">{}</button>"#,
                active, asset.name, asset.name,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_panels(assets: &[AssetFrames]) -> String {
    assets
        .iter()
        .enumerate()
        .map(|(index, asset)| build_panel(index, asset))
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_panel(index: usize, asset: &AssetFrames) -> String {
    let hidden = if index == 0 { "" } else { " hidden" };
    let states_html = build_state_sections(asset);
    format!(
        r#"<section class="panel{}" data-asset="{}">
<div class="controls">
<button class="btn-play" data-playing="false">&#9654; Play</button>
<button class="btn-step">Step &raquo;</button>
<select class="speed-select">
<option value="0.5">0.5x</option>
<option value="1" selected>1x</option>
<option value="2">2x</option>
</select>
</div>
<div class="preview-strip" data-asset="{}"></div>
{}</section>"#,
        hidden, asset.name, asset.name, states_html,
    )
}

fn build_state_sections(asset: &AssetFrames) -> String {
    asset
        .sorted_states
        .iter()
        .map(|(state, indices)| {
            let frames_html: String = indices
                .iter()
                .enumerate()
                .map(|(offset, _)| {
                    let src = format!("{}/svg/{}_{}.svg", asset.name, state, offset,);
                    format!(
                        r#"<div class="frame-cell" data-asset="{}" data-state="{}" data-frame="{}">
<img src="{}" alt="{} {} frame {}">
<span class="frame-label">#{}</span>
</div>"#,
                        asset.name, state, offset, src, asset.name, state, offset, offset,
                    )
                })
                .collect();
            format!(
                r#"<div class="state-group">
<h3>{}</h3>
<div class="frame-grid">{}</div>
</div>"#,
                state, frames_html,
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const STYLE_BLOCK: &str = r#"<style>
*,*::before,*::after{box-sizing:border-box;margin:0;padding:0}
body{background:#1a1a2e;color:#e0e0e0;font-family:system-ui,-apple-system,sans-serif;
padding:2rem;max-width:1200px;margin:0 auto}
header{text-align:center;margin-bottom:2rem}
h1{font-size:1.5rem;letter-spacing:0.04em;color:#f0f0f0}
.subtitle{font-size:0.85rem;color:#888;margin-top:0.25rem}
#tabs{display:flex;gap:0.5rem;justify-content:center;margin-bottom:1.5rem}
.tab{background:#2a2a4a;border:1px solid #3a3a5a;color:#ccc;padding:0.5rem 1.25rem;
border-radius:6px;cursor:pointer;font-size:0.9rem;transition:all 0.15s}
.tab:hover{background:#3a3a5a}
.tab.active{background:#4a4a7a;color:#fff;border-color:#6a6aaa}
.panel{animation:fadeIn 0.2s ease}
.panel[hidden]{display:none}
@keyframes fadeIn{from{opacity:0}to{opacity:1}}
.controls{display:flex;gap:0.5rem;align-items:center;margin-bottom:1.25rem;
padding:0.75rem;background:#222244;border-radius:8px}
.controls button,.controls select{background:#3a3a5a;border:1px solid #4a4a6a;
color:#ddd;padding:0.4rem 0.75rem;border-radius:4px;cursor:pointer;font-size:0.85rem}
.controls button:hover,.controls select:hover{background:#4a4a7a}
.preview-strip{display:flex;align-items:center;justify-content:center;gap:1rem;
min-height:200px;background:#12122a;border-radius:8px;margin-bottom:1.5rem;padding:1rem}
.preview-strip img{image-rendering:pixelated;width:192px;height:auto;
border:2px solid #3a3a5a;border-radius:4px;background:#0a0a1a}
.state-group{margin-bottom:1.5rem}
.state-group h3{font-size:0.95rem;color:#aaa;margin-bottom:0.5rem;
border-bottom:1px solid #2a2a4a;padding-bottom:0.25rem}
.frame-grid{display:flex;flex-wrap:wrap;gap:0.75rem}
.frame-cell{display:flex;flex-direction:column;align-items:center;gap:0.25rem;
padding:0.5rem;background:#1e1e3a;border-radius:6px;border:2px solid transparent;
cursor:pointer;transition:border-color 0.15s}
.frame-cell:hover{border-color:#5a5a8a}
.frame-cell.active{border-color:#7a7aba}
.frame-cell img{image-rendering:pixelated;width:96px;height:auto}
.frame-label{font-size:0.7rem;color:#666}
</style>"#;

const SCRIPT_BLOCK: &str = r#"
(function(){
  const tabs = document.querySelectorAll('.tab');
  const panels = document.querySelectorAll('.panel');

  tabs.forEach(function(tab){
    tab.addEventListener('click', function(){
      tabs.forEach(function(t){t.classList.remove('active')});
      panels.forEach(function(p){p.hidden = true});
      tab.classList.add('active');
      var target = tab.getAttribute('data-asset');
      document.querySelector('.panel[data-asset="'+target+'"]').hidden = false;
    });
  });

  panels.forEach(function(panel){
    var asset = panel.getAttribute('data-asset');
    var strip = panel.querySelector('.preview-strip');
    var playBtn = panel.querySelector('.btn-play');
    var stepBtn = panel.querySelector('.btn-step');
    var speedSel = panel.querySelector('.speed-select');
    var cells = panel.querySelectorAll('.frame-cell');
    var frames = [];
    var currentIndex = 0;
    var intervalId = null;

    cells.forEach(function(cell){
      frames.push(cell.querySelector('img').src);
    });

    function showFrame(idx){
      currentIndex = idx % frames.length;
      strip.innerHTML = '<img src="'+frames[currentIndex]+'" alt="preview">';
      cells.forEach(function(c,ci){
        c.classList.toggle('active', ci === currentIndex);
      });
    }

    cells.forEach(function(cell, ci){
      cell.addEventListener('click', function(){
        stopAnim();
        showFrame(ci);
      });
    });

    function stopAnim(){
      if(intervalId){clearInterval(intervalId);intervalId=null;}
      playBtn.innerHTML = '&#9654; Play';
      playBtn.setAttribute('data-playing','false');
    }

    function startAnim(){
      var speed = parseFloat(speedSel.value);
      var ms = Math.round(200 / speed);
      intervalId = setInterval(function(){
        showFrame(currentIndex + 1);
      }, ms);
      playBtn.innerHTML = '&#9646;&#9646; Pause';
      playBtn.setAttribute('data-playing','true');
    }

    playBtn.addEventListener('click', function(){
      if(playBtn.getAttribute('data-playing') === 'true'){
        stopAnim();
      } else {
        startAnim();
      }
    });

    stepBtn.addEventListener('click', function(){
      stopAnim();
      showFrame(currentIndex + 1);
    });

    speedSel.addEventListener('change', function(){
      if(playBtn.getAttribute('data-playing') === 'true'){
        stopAnim();
        startAnim();
      }
    });

    if(frames.length > 0) showFrame(0);
  });
})();
"#;
