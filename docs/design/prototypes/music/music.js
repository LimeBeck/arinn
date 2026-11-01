// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: CC-BY-SA-4.0
// Offline visual design fixtures. No media, accounts, storage or service calls.
(() => {
  'use strict';
  const content = document.querySelector('#content');
  const overlay = document.querySelector('#overlay');
  const player = document.querySelector('#player');
  const sources = [
    ['local','Local library','Ready · 42 demo tracks'],
    ['yandex','Yandex Music','Integration route under review'],
    ['spotify','Spotify','Service permission required'],
    ['soundcloud','SoundCloud','Not connected'],
    ['youtube','YouTube Music','Playback route under review']
  ];
  const albums = [
    ['Glass Rivers','Northline','a'],['Slow Morning','Mira Vale','b'],
    ['After the Rain','Parallel Coast','c'],['Open Fields','August Atlas','d'],
    ['Night Letters','Northline','e']
  ];
  const tracks = [
    ['First Light','3:42'],['Glass Rivers','4:18'],['The Long Way Home','5:06'],
    ['Soft Edges','3:57'],['Still Water','4:31'],['Northbound','3:28']
  ];
  const states = {
    loading:['Loading your music','Your catalogue will appear here.'],
    empty:['No music yet','Add a music folder to start building your library.'],
    'no-results':['No matches','Try a different title or artist in this source.'],
    scanning:['Finding your music','18 of 42 demo tracks indexed. You can keep browsing.'],
    offline:['Music folder unavailable','Your saved music stays here. Reconnect the drive to play it.'],
    unreadable:['This track cannot be opened','Check access to the file, then try again.'],
    codec:['This format cannot be played','Choose another track or check playback support.'],
    expired:['Connect again','Your session has expired. Your current music view is preserved.'],
    subscription:['Subscription required','This track needs an eligible account. Other music is still available.'],
    restricted:['Track unavailable','This track is not available for playback from this source.'],
    preview:['Preview only','Only a short preview is available. Full playback is not offered.'],
    'no-device':['Choose a playback device','No playback device is ready. Nothing has started.'],
    rate:['Please wait a moment','This source is receiving too many requests. Try again later.'],
    unavailable:['Source unavailable','Your music from other sources is still available.'],
    restart:['Reconnecting to your music','The source is restarting. Your selection is preserved.'],
    stale:['Queue changed','The queue was updated elsewhere. Refresh before making changes.'],
    pending:['Connecting…','Complete connection in the approved service flow. This preview never opens an account.'],
    cancelled:['Connection cancelled','Nothing was connected. You can try again when you are ready.']
  };
  let s = {screen:'catalogue',source:'local',tab:'Albums',album:0,query:'',scene:'catalogue'};
  let session = null;
  let serial = 0;
  const queueFor = album => tracks.map((_,track) => ({track,album,key:++serial}));
  const playlists = ['Evening rotation','Weekend paths','Quiet hours','On the move','Favourites'];
  let queue = queueFor(0);
  function selectEntry(entry) {if(session && entry) Object.assign(session,{track:entry.track,album:entry.album,key:entry.key});}
  let history = [];
  let modal = null;
  let modalOpener = null;
  const esc = value => String(value).replace(/[&<>"']/g,c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  const sourceName = id => sources.find(x => x[0] === id)[1];
  const button = (id,label,action,arg='',cls='') => `<button id="${id}" class="${cls}" data-action="${action}" data-arg="${esc(arg)}">${label}</button>`;
  const art = (index,extra='') => `<div aria-hidden="true" class="art ${albums[index % albums.length][2]} ${extra}"></div>`;
  function focus(id) { const el = id && document.getElementById(id); if (el) el.focus({preventScroll:true}); else (overlay.querySelector('button,input') || content.querySelector('button,input'))?.focus({preventScroll:true}); }
  function navigate(patch) {
    history.push({state:{...s},focus:document.activeElement.id,scroll:content.scrollTop});
    s = {...s,...patch}; content.scrollTop = 0; render();
  }
  function back() {
    if (modal) { modal=null; drawModal(); focus(modalOpener); return; }
    const prev = history.pop();
    if (prev) {s=prev.state;render(prev.focus);content.scrollTop=prev.scroll;}
    else {navigate({screen:s.screen === 'home'?'catalogue':'home'});}
  }
  function openModal(kind,arg) {modalOpener=document.activeElement.id;modal={kind,arg};drawModal();}
  function rows(indices=tracks.map((_,i)=>i),queueMode=false) {
    return `<div class="list">${indices.map((entry,n)=>{const i=queueMode?entry.track:entry,album=queueMode?entry.album:s.album,current=queueMode?session?.key===entry.key:session?.track===i&&session?.album===album;return button(`track-${n}`,`<span class="number">${current?'♪':String(n+1).padStart(2,'0')}</span><span class="meta"><h3>${esc(tracks[i][0])}</h3><p>${s.scene==='metadata'?'Unknown artist · Unknown album':esc(albums[album][1])+' · '+esc(albums[album][0])}</p></span><span class="duration">${tracks[i][1]}</span>`,queueMode?'queue-action':'track',queueMode?n:i,`track ${current?'current':''}`);}).join('')}</div>`;
  }
  function grid() {
    return `<div class="grid">${albums.map((a,i)=>button(`album-${i}`,`${art(i,s.tab==='Artists'?'artist':'')}<h3>${esc(s.tab==='Artists'?a[1]:s.tab==='Playlists'?playlists[i]:a[0])}</h3><p>${s.tab==='Artists'?'Artist':s.tab==='Playlists'?'Local library · 6 tracks':esc(a[1])+' · Album'}</p>`,s.tab==='Artists'?'artist':'detail',i,'card')).join('')}</div>`;
  }
  function render(preferred) {
    const isState = Boolean(states[s.scene]);
    const header = `<header>${button('back','‹','back','','quiet')}<div class="title"><div class="eyebrow">${s.screen==='home'?'ARINN':esc(sourceName(s.source))}</div><h1>${s.screen==='player'?'Now playing':s.screen==='queue'?'Play queue':s.screen==='home'?'Home':'Music'}</h1></div>${button('sources',esc(sourceName(s.source))+' ⌄','sources','','pill')}${s.source==='local'?button('search','Search','search','','pill'):''}${button('settings','Settings','settings','','quiet')}</header>`;
    let html = '';
    if (s.screen==='home') html=`<div class="state"><div class="eyebrow">YOUR SPACE</div><h2>Make yourself at home.</h2><p>Your music keeps its place.</p>${button('music','Open music','music','','primary pill')}</div>`;
    else if (s.screen==='player' && session) {
      html=`<div class="now">${art(session.album)}<div class="info"><div class="eyebrow">${esc(sourceName(session.source))} · Demo session</div><h1>${esc(tracks[session.track][0])}</h1><p>${esc(albums[session.album][1])}</p><p class="muted">${esc(albums[session.album][0])}</p><div class="timeline"><div class="bar"><div class="fill"></div></div><div class="gap"><span>1:12</span><span>${tracks[session.track][1]}</span></div></div><div class="gap transport">${button('previous','Previous','previous')}${button('pause',session.paused?'Play':'Pause','pause','','primary')}${button('next','Next','next')}</div><div class="gap">${button('queue','View queue','queue','','pill')}${button('seek','Seek +10s','seek','','quiet')}</div></div></div>`;
    } else if (s.screen==='queue') {
      html=`<div class="section-title"><h2>Up next</h2><span class="small">${queue.length} tracks · ${esc(sourceName(session?.source || s.source))}</span></div><p class="small">Select a track for queue actions.</p>${rows(queue,true)}`;
    } else if (s.screen==='search') {
      html=`<input id="query" class="search-field" placeholder="Search ${esc(sourceName(s.source))}" aria-label="Search music" value="${esc(s.query)}">${button('keyboard','On-screen keyboard','keyboard','','pill')}<div class="section-title"><h2>${s.query?'Results for “'+esc(s.query)+'”':'Find your next favourite'}</h2><span class="small">${esc(sourceName(s.source))}</span></div>${s.query?(tracks.some(t=>t[0].toLowerCase().includes(s.query.toLowerCase()))?rows(tracks.map((_,i)=>i).filter(i=>tracks[i][0].toLowerCase().includes(s.query.toLowerCase()))):'<p class="small">No matches. Try a different title.</p>'):'<p class="small">Search by track title in this synthetic catalogue.</p>'}`;
    } else if (s.screen==='artist') {
      html=`<div class="detail">${art(s.album,'artist')}<div><div class="eyebrow">ARTIST</div><h1>${esc(albums[s.album][1])}</h1><p>Albums in your local library</p></div></div><div class="grid">${button('release',`${art(s.album)}<h3>${esc(albums[s.album][0])}</h3><p>6 tracks · Album</p>`,'detail',s.album,'card')}</div>`;
    } else if (s.screen==='detail') {
      html=`<div class="detail">${art(s.album)}<div><div class="eyebrow">${s.tab==='Playlists'?'PLAYLIST':'ALBUM'} · ${esc(sourceName(s.source))}</div><h1>${esc(s.tab==='Playlists'?playlists[s.album]:albums[s.album][0])}</h1><p>${esc(albums[s.album][1])} · 6 tracks · 25 minutes</p><div class="gap">${button('play-album','Play','play',0,'primary pill')}${button('add-album','Add to queue','add','','pill')}</div></div></div>${rows()}`;
    } else if (isState) {
      const [title,description]=states[s.scene];
      html=s.scene==='loading'?`<div class="section-title"><h2>${title}</h2></div><div class="grid">${Array(5).fill('<div class="skeleton"></div>').join('')}</div>`:`<div class="state"><div class="symbol">${s.scene==='scanning'?'…':'○'}</div><h2>${title}</h2><p>${description}</p><div class="gap">${button('recover',s.scene==='empty'?'Choose folders':s.scene==='stale'?'Refresh queue':s.scene==='pending'?'Cancel':s.scene==='expired'?'Connect again':'Back to music','recover','','primary pill')}${button('state-sources','Other sources','sources','','pill')}</div></div>`;
    } else if (s.source!=='local') {
      html=`<div class="state"><div class="symbol">♪</div><h2>${s.source==='soundcloud'?'Connect SoundCloud':'This source is not ready yet'}</h2><p>${s.source==='soundcloud'?'Connect your account when an approved integration is available.':esc(sources.find(x=>x[0]===s.source)[2])+'. You can keep browsing your local library.'}</p><div class="gap">${button('connect',s.source==='soundcloud'?'Connection options':'View availability','settings','','primary pill')}${button('choose-local','Browse local music','local','','pill')}</div></div>`;
    } else {
      html=`<nav class="tabs" aria-label="Music categories">${['Artists','Albums','Playlists','Tracks'].map(t=>button(`tab-${t}`,t,'tab',t,'pill '+(s.tab===t?'selected':''))).join('')}</nav>${s.scene==='missing-art'?'<div class="notice">Some artwork is unavailable. Your music is still here.</div>':''}<div class="section-title"><h2>${s.tab==='Albums'?'Your albums':s.tab}</h2><span class="small">Local library · Demo catalogue</span></div>${s.tab==='Tracks'?rows():grid()}`;
    }
    content.innerHTML=header+html;
    drawPlayer();drawModal();focus(preferred);
  }
  function drawPlayer() {
    if(!session){player.innerHTML='';return;}
    player.innerHTML=`${art(session.album)}${button('mini-title',`<h3>${esc(tracks[session.track][0])}</h3><p>${esc(albums[session.album][1])} · ${esc(sourceName(session.source))}</p>`,'player','','identity')}${button('mini-previous','Previous','previous','','quiet')}${button('mini-pause',session.paused?'Play':'Pause','pause','','primary pill')}${button('mini-next','Next','next','','quiet')}${button('mini-queue','Queue','queue','','pill')}`;
  }
  function drawModal() {
    content.inert=Boolean(modal);player.inert=Boolean(modal);
    if(!modal){overlay.innerHTML='';return;}
    let html='',type='sheet';
    if(modal.kind==='sources') html=`<h2>Your music sources</h2><p>One place for your music. Choose where to browse.</p>${sources.map(([id,name,status],i)=>button(`source-${id}`,`<span class="source-icon">${i===0?'♪':name[0]}</span><span class="grow">${name}<span>${status}</span></span>${s.source===id?'✓':'›'}`,'source',id,'source-row')).join('')}${button('close-sheet','Done','back','','pill')}`;
    if(modal.kind==='settings') html=`<h2>${esc(sourceName(s.source))}</h2><p>${s.source==='local'?'Your local music folders. This preview never reads your files.':esc(sources.find(x=>x[0]===s.source)[2])+'. No account will be opened by this preview.'}</p>${s.source==='local'?'<div class="notice">Music collection <span class="tag">Demo root</span></div>':'<div class="notice">Account not connected</div>'}<div class="gap">${button('close-sheet','Back to music','back','','primary pill')}</div>`;
    if(modal.kind==='keyboard') html=`<h2>Search music</h2><p id="typed">${esc(s.query)||'Enter a title'}</p><div class="keyboard">${'ABCDEFGHIJKLMNOPQRSTUVWXYZ '.split('').map((c,i)=>button('key-'+i,c===' '?'Space':c,'key',c)).join('')}</div><div class="gap">${button('erase','Delete','erase')}${button('keyboard-done','Done','keyboard-done','','primary')}</div>`;
    if(modal.kind==='queue-action') html=`<h2>${esc(tracks[queue[modal.arg].track][0])}</h2><p>Queue actions · ${esc(sourceName(session?.source||s.source))}</p><div class="list">${button('queue-play','Play now','queue-play',modal.arg)}${button('queue-up','Move earlier','queue-up',modal.arg)}${button('queue-remove','Remove from queue','queue-remove',modal.arg)}${button('close-sheet','Cancel','back','','quiet')}</div>`;
    if(modal.kind==='replace'){type='dialog';html=`<h2>Replace the current music?</h2><p>Playing this selection replaces the current queue. Your ${esc(sourceName(session.source))} session will stop.</p><div class="gap">${button('replace-cancel','Cancel','back','','pill')}${button('replace-confirm','Replace and play','replace',modal.arg,'primary pill')}</div>`;}
    if(modal.kind==='message'){type='dialog';html=`<h2>${esc(modal.arg)}</h2><p>This is an interaction preview. No audio is played.</p>${button('close-sheet','Done','back','','primary pill')}`;}
    overlay.innerHTML=`<div class="scrim"><section class="${type}" role="dialog" aria-modal="true" aria-label="Music options">${html}</section></div>`;
    focus(modal.kind==='replace'?'replace-cancel':null);
  }
  function start(track) {if(s.source!=='local'){openModal('settings');return;}session={source:s.source,album:s.album,track:Number(track),paused:false};queue=queueFor(s.album);selectEntry(queue[Number(track)]);modal=null;drawModal();drawPlayer();render(document.activeElement.id);}
  function play(track) {if(s.source!=='local'){openModal('settings');return;}if(session && (session.source!==s.source || session.album!==s.album)){openModal('replace',track);}else start(track);}
  function applyScene(scene) {
    s={screen:'catalogue',source:'local',tab:'Albums',album:0,query:'',scene};history=[];modal=null;session=null;queue=queueFor(0);
    if(['player','queue','home'].includes(scene)){session={source:'local',album:0,track:1,paused:false};s.screen=scene;selectEntry(queue[1]);}
    if(scene==='sources'){render();openModal('sources');return;}
    if(scene==='search'){s.screen='search';s.query='Glass';}
    if(scene==='detail')s.screen='detail';
    if(scene==='replace'){s.screen='detail';s.album=1;session={source:'local',album:0,track:1,paused:false};selectEntry(queue[1]);render();openModal('replace',0);return;}
    if(scene==='account'){s.source='soundcloud';}
    if(scene==='missing-art'||scene==='metadata')s.tab='Tracks';
    render();
  }
  document.addEventListener('click',e=>{
    const b=e.target.closest('[data-action]');if(!b)return;
    const a=b.dataset.action,arg=b.dataset.arg;
    if(a==='back')back();
    else if(a==='music'){if(history.length)back();else navigate({screen:'catalogue',scene:'catalogue'});}
    else if(a==='sources')openModal('sources');
    else if(a==='settings')openModal('settings');
    else if(a==='source'){modal=null;drawModal();navigate({source:arg,screen:'catalogue',scene:'catalogue'});}
    else if(a==='local')navigate({source:'local',screen:'catalogue',scene:'catalogue'});
    else if(a==='tab'){s.tab=arg;render('tab-'+arg);}
    else if(a==='detail'||a==='artist')navigate({screen:a,album:Number(arg)});
    else if(a==='search'){if(s.source==='local')navigate({screen:'search',query:'',scene:'catalogue'});else openModal('settings');}
    else if(a==='keyboard')openModal('keyboard');
    else if(a==='key'||a==='erase'){s.query=a==='key'?s.query+arg:s.query.slice(0,-1);overlay.querySelector('#typed').textContent=s.query||'Enter a title';}
    else if(a==='keyboard-done'){modal=null;render('query');}
    else if(a==='track')play(Number(arg));
    else if(a==='play')play(arg);
    else if(a==='replace')start(arg);
    else if(a==='pause'&&session){const id=b.id;session.paused=!session.paused;s.screen==='player'?render(id):(drawPlayer(),focus(id));}
    else if(a==='next'||a==='previous'){if(session&&queue.length){const id=b.id,index=queue.findIndex(entry=>entry.key===session.key);const target=index<0?0:(index+(a==='next'?1:queue.length-1))%queue.length;selectEntry(queue[target]);render(id);}}
    else if(a==='player')navigate({screen:'player',scene:'catalogue'});
    else if(a==='queue')navigate({screen:'queue',source:session?.source||s.source,album:session?.album||0,scene:'catalogue'});
    else if(a==='queue-action')openModal('queue-action',Number(arg));
    else if(a==='queue-play'){selectEntry(queue[Number(arg)]);modal=null;render('track-'+arg);}
    else if(a==='queue-up'){const i=Number(arg);if(i>0)[queue[i-1],queue[i]]=[queue[i],queue[i-1]];modal=null;render('track-'+Math.max(0,i-1));}
    else if(a==='queue-remove'){queue.splice(Number(arg),1);modal=null;render('track-'+Math.min(Number(arg),queue.length-1));}
    else if(a==='add'){if(session&&session.source!==s.source)openModal('message','Choose the current source to add music');else{queue.push(...queueFor(s.album));openModal('message','Added to the demo queue');}}
    else if(a==='seek')openModal('message','Seek preview');
    else if(a==='recover'){if(s.scene==='pending'){s.scene='cancelled';render();}else if(s.scene==='expired'||s.scene==='empty')openModal('settings');else{s.scene='catalogue';render();}}
  });
  document.addEventListener('input',e=>{if(e.target.id==='query')s.query=e.target.value;});
  document.addEventListener('keydown',e=>{
    if(e.key==='Escape'||(e.key==='Backspace'&&e.target.tagName!=='INPUT')){e.preventDefault();back();return;}
    if(e.target.tagName==='SELECT')return;
    if(e.target.tagName==='INPUT'&&e.key!=='ArrowDown'&&e.key!=='Enter')return;
    if(e.key==='Enter'&&e.target.id==='query'){e.preventDefault();render('keyboard');return;}
    if(e.key.toLowerCase()==='h'&&e.target.tagName!=='INPUT'){e.preventDefault();if(modal){modal=null;drawModal();}navigate({screen:'home'});return;}
    if(!['ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Tab'].includes(e.key))return;
    const scope=modal?overlay:document.querySelector('#stage');
    const els=[...scope.querySelectorAll('button:not(:disabled),input')].filter(x=>!x.closest('[inert]'));
    if(e.key==='Tab'&&modal){e.preventDefault();const i=els.indexOf(document.activeElement);els[(i+(e.shiftKey?els.length-1:1))%els.length]?.focus();return;}
    if(e.key==='Tab')return;
    e.preventDefault();const origin=document.activeElement.getBoundingClientRect();
    const dx=e.key==='ArrowLeft'?-1:e.key==='ArrowRight'?1:0,dy=e.key==='ArrowUp'?-1:e.key==='ArrowDown'?1:0;
    const ox=origin.x+origin.width/2,oy=origin.y+origin.height/2;
    const next=els.filter(x=>x!==document.activeElement).map(x=>{const r=x.getBoundingClientRect(),xv=r.x+r.width/2-ox,yv=r.y+r.height/2-oy;return {x,along:xv*dx+yv*dy,cross:Math.abs(xv*dy-yv*dx)};}).filter(x=>x.along>1).sort((a,b)=>(a.along+a.cross*3)-(b.along+b.cross*3))[0];
    if(next)next.x.focus();
  });
  const scenes=['catalogue','sources','detail','search','player','queue','home','account','replace','missing-art','metadata',...Object.keys(states)];
  document.querySelector('#scene').innerHTML=scenes.map(x=>`<option>${x}</option>`).join('');
  document.querySelector('#scene').addEventListener('change',e=>applyScene(e.target.value));
  function resize(){document.documentElement.style.setProperty('--scale',Math.min(innerWidth/1920,innerHeight/1080));}
  addEventListener('resize',resize);resize();
  const params=new URLSearchParams(location.search);document.body.classList.toggle('clean',params.get('controls')==='0');
  const scene=scenes.includes(params.get('scene'))?params.get('scene'):'catalogue';document.querySelector('#scene').value=scene;applyScene(scene);
})();
