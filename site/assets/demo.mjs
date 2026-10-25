// Silent inline autoplay, with ordinary controls when browser policy blocks it.
const video=document.getElementById('tracking-demo');
const toggle=document.getElementById('demo-toggle');
if (video && toggle) {
  const label=()=>{toggle.textContent=video.paused?'Play demo ↗':'Pause demo';};
  video.muted=true;
  video.defaultMuted=true;
  video.addEventListener('play',label);
  video.addEventListener('pause',label);
  toggle.addEventListener('click',async()=>{
    if (!video.paused) video.pause();
    else {try {await video.play();} catch {label();}}
  });
  video.play().catch(label);
}
