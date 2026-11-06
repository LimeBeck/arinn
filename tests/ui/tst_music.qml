// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/music" as Music
TestCase {
    id: suite; name: "NativeMusic"; width: 1920; height: 1080; visible: true; when: windowShown
    Component {
        id: catalogFactory
        QtObject {
            property var sources: [{id:"local",name:"Fixture music"}]
            property string sourceId: "local"
            property var entries: [{id:"track-1",kind:"track",title:"Fixture track",artist:"Fixture artist",available:true}]
            property var state: ({account:"local",facets:["tracks"],actions:["play"]})
            property var activeState: ({session:"session",playback:"playing",current:{occurrence:"occurrence",track:{id:"track-0",title:"Existing track",artist:"Fixture artist",duration_us:10000000}},actions:["pause","next","stop","seek"],upcoming:[]})
            property string activeSourceName: "Fixture music"
            property string activeSourceId: ""
            property string errorText: ""
            property string notice: ""
            property string playbackProblem: ""
            property bool busy: false
            property bool transportBusy: busy
            property bool more: false
            property var commands: []
            function choose(id) { sourceId=id; }
            property var browsed: []
            property var played: []
            function browse(facet,parent,search) {browsed=browsed.concat([parent]);}
            signal pageFailed()
            property int loads: 0
            // As the catalogue: a page in flight makes it busy.
            property bool busyOnLoad: false
            function loadMore() {if(busy)return;loads++;if(busyOnLoad)busy=true;}
            function command(action,id,occurrence,position) {commands=commands.concat([action]);played=played.concat([id||""]);}
            property var positions: []
            property var occurrences: []
            // As the catalogue: a command it sends makes it busy (#802).
            property bool busyOnTransport: false
            function transport(action,id,occurrence,position) {commands=commands.concat([action]);positions=positions.concat([position]);occurrences=occurrences.concat([occurrence||""]);if(busyOnTransport)busy=true;}
        }
    }
    // The Music root over every source (#800), as MusicRoom presents it.
    Component {
        id: roomFactory
        QtObject {
            function card(provider,id,kind,title) {return {key:provider+"\n"+id,id:id,kind:kind,title:title,provider:provider,providerName:provider==="local"?"Fixture music":"Fixture service",local:provider==="local",available:true};}
            property var sources: [{id:"local",name:"Fixture music",local:true,status:"connected",reason:"",actions:["scan"],busy:false,browsable:true},
                                   {id:"account",name:"Fixture service",local:false,status:"connected",reason:"",actions:["scan","disconnect"],busy:false,browsable:true}]
            property var shelves: [{id:"stations",title:"Stations",kind:"station",entries:[card("account","station:mix","station","Fixture mix")]},
                                   {id:"albums",title:"Albums",kind:"album",entries:[card("local","album:1","album","First"),card("account","album:1","album","Same id"),card("local","album:2","album","Second")]}]
            property bool connected: true
            property bool loading: false
            property bool dated: false
            property var commands: []
            signal listChanged()
            function list(shelf,source,alphabetical) {const found=shelves.find(item=>item.id===shelf);const entries=found?found.entries:[];return entries.filter(entry=>!source||entry.provider===source);}
            // Pages the All list still has to read; each loadMore delivers one.
            property var pending: []
            property int loads: 0
            function more(shelf,source) {return pending.length>0;}
            function loadMore(shelf,source) {loads++;const next=pending[0];pending=pending.slice(1);
                if(next.length)shelves=shelves.map(item=>item.id===shelf?Object.assign({},item,{entries:item.entries.concat(next)}):item);
                listChanged();}
            // As MusicRoom: a command in flight republishes the rows.
            function sourceCommand(source,action) {commands=commands.concat([source+":"+action]);sources=sources.map(row=>Object.assign({},row,{busy:row.id===source}));}
            // As Main stores the choice and MusicRoom lists an off source last (#842).
            function setSourceEnabled(source,enabled) {commands=commands.concat([source+":"+(enabled?"on":"off")]);
                const rows=sources.map(row=>row.id!==source?row:Object.assign({},row,enabled?{status:"connected",actions:row.offActions||[],browsable:true}:{status:"off",offActions:row.actions,actions:[],browsable:false}));
                sources=rows.filter(row=>row.status!=="off").concat(rows.filter(row=>row.status==="off"));}
            // Find over every source (#801): the query is kept; tests set the results.
            property var searchShelves: []
            property var searchPending: []
            property string searchQuery: ""
            property var searches: []
            function search(query) {searchQuery=query;searches=searches.concat([query]);}
            function stable(shown,desired) {const keys=desired.map(entry=>entry.key);const kept=shown.filter(entry=>keys.includes(entry.key)).map(entry=>desired[keys.indexOf(entry.key)]);return kept.concat(desired.filter(entry=>!kept.some(item=>item.key===entry.key)));}
        }
    }
    Component {id:roomViewFactory;Music.MusicView {width:1920;height:1080}}
    Component {id:viewFactory;Music.MusicView {width:1920;height:1080}}
    // A scope above the view, such as the Quick menu, holding focus.
    Component {id:scopeFactory;FocusScope {width:10;height:10;Item {objectName:"scopeItem";width:10;height:10;focus:true}}}
    // As the shell places it: no size given (#756 M6, M7).
    Component {id:shellViewFactory;Music.MusicView {}}
    Component {id:artworkFactory;Music.MusicArtwork {width:128;height:128;retryInterval:10}}
    Component {id:homeFactory;Ui.HomeView {}}
    Component {id:rowFactory;Ui.HomeRow {width:1600;entries:[{id:"music",title:"Fixture",secondaryIcon:"pause",secondaryText:"Pause"}]}}
    function test_artwork_retries_are_bounded_and_reset_for_a_new_source() {
        const art=createTemporaryObject(artworkFactory,suite,{source:Qt.resolvedUrl("missing-synthetic-music-artwork.png")});verify(art);
        tryCompare(art,"attempts",6);compare(art.loaded,false);wait(50);compare(art.attempts,6);
        compare(findChild(art,"musicArtworkRetry").interval,640);
        art.source=Qt.resolvedUrl("fixture-glow.svg");tryCompare(art,"loaded",true);compare(art.attempts,0);
        tryVerify(()=>{const pixels=grabImage(art);return pixels.red(64,64)>150 && pixels.blue(64,64)<100;});
        art.retryInterval=1000;art.attempts=4;compare(findChild(art,"musicArtworkRetry").interval,16000);
        art.attempts=5;compare(findChild(art,"musicArtworkRetry").interval,30000);
        art.visible=false;art.source=Qt.resolvedUrl("missing-synthetic-music-artwork.png");wait(100);compare(art.attempts,0);
    }
    function test_elapsed_duration_and_catalogue_position_are_readable() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        compare(findChild(view,"musicListPosition").text,"1 of 1");
        catalog.activeState={playback:"playing",position_us:65000000,current:{track:{duration_us:185000000}},actions:[]};
        view.openPlayer();tryCompare(findChild(view,"musicElapsed"),"text","1:05");
        compare(findChild(view,"musicDuration").text,"3:05");
        // #802 (M25): a track longer than an hour carries hours, elapsed too.
        catalog.activeState={playback:"playing",position_us:65000000,current:{track:{duration_us:4503000000}},actions:[]};
        tryCompare(findChild(view,"musicElapsed"),"text","0:01:05");
        compare(findChild(view,"musicDuration").text,"1:15:03");
    }
    function test_long_title_fits_two_lines_without_colliding_with_controls() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={playback:"playing",current:{track:{id:"fixture",title:"An extended synthetic title from the fixture album",artist:"Synthetic artist",album:"Synthetic album",duration_us:185000000}},actions:["pause","seek"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,mode:"player"});verify(view);wait(0);
        const title=findChild(view,"musicTrackTitle"),info=findChild(view,"musicInfoColumn"),controls=findChild(view,"musicControlsB");
        tryVerify(()=>!title.truncated);verify(title.font.pixelSize>=64);
        verify(info.mapToItem(view,0,info.height).y+24<=controls.mapToItem(view,0,0).y);
    }
    function test_connected_actions_have_distinct_geometry_and_remote_navigation() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"connected",facets:["selections"],actions:["scan","disconnect"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        const refresh=findChild(view,"musicRescan"),disconnect=findChild(view,"musicDisconnect");
        verify(refresh.visible && disconnect.visible);
        verify(refresh.x+refresh.width<=disconnect.x || disconnect.x+disconnect.width<=refresh.x || refresh.y+refresh.height<=disconnect.y || disconnect.y+disconnect.height<=refresh.y);
        Ui.Focus.take(refresh);keyClick(Qt.Key_Right);tryCompare(disconnect,"activeFocus",true);
        keyClick(Qt.Key_Left);tryCompare(refresh,"activeFocus",true);
    }
    // Now playing has no page title: the state is in the eyebrow, and the
    // Ember dot only while playing (#840).
    function test_now_playing_state_is_in_the_eyebrow() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,mode:"player"});verify(view);wait(0);
        const heading=findChild(view,"musicHeading"),context=findChild(view,"musicPlayerContext"),dot=findChild(view,"musicPlayerDot");
        verify(heading);verify(context);verify(dot);
        compare(heading.visible,false);
        const station={id:"station:wave",title:"Fixture wave"};
        for(const state of [{playback:"playing",text:"Fixture music",dot:true},{playback:"paused",text:"Fixture music · paused",dot:false},
                            {playback:"playing",station:station,text:"Fixture wave · Fixture music",dot:true},{playback:"paused",station:station,text:"Fixture wave · paused",dot:false},
                            {playback:"loading",station:station,text:"Fixture wave · loading",dot:false},{playback:"stopped",error:"Unavailable",text:"Fixture music · stopped",dot:false}]) {
            catalog.activeState={playback:state.playback,error:state.error,station:state.station,actions:[],upcoming:[]};
            tryCompare(context,"text",state.text);compare(dot.visible,state.dot);compare(heading.visible,false);
        }
        // Without a context or a source the state stands alone.
        catalog.activeSourceName="";
        tryCompare(context,"text","Stopped");
        catalog.activeState={playback:"paused",actions:[],upcoming:[]};tryCompare(context,"text","Paused");
        catalog.activeState={playback:"loading",actions:[],upcoming:[]};tryCompare(context,"text","Loading music…");
        catalog.activeState={playback:"playing",actions:[],upcoming:[]};tryCompare(context,"text","Now playing");
        // The Queue sheet opens over Now playing: no title either.
        view.mode="queue";wait(0);compare(heading.visible,false);
        view.mode="catalogue";tryCompare(heading,"visible",true);
    }
    // Lockup and the shell's status area, as frame mh-03 (#840).
    function test_now_playing_has_the_screen_chrome() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);wait(0);
        compare(view.screenStatus,true);
        view.openPlayer();wait(0);compare(view.mode,"player");
        const lockup=findChild(view,"musicPlayerLockup");verify(lockup);verify(lockup.visible);
        compare(lockup.x,Ui.Theme.safeLeft);compare(lockup.height,40);
        compare(view.screenStatus,true);
        view.openMore();tryCompare(view,"screenStatus",false);
        view.moreOpen=false;tryCompare(view,"screenStatus",true);
        view.openQueue();tryCompare(view,"screenStatus",false);
    }
    // Previous, play/pause, next and More are circles (#840, MH-3).
    function test_round_buttons_are_circles() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        for(const [name,size] of [["musicPrevious",96],["musicPlayPause",120],["musicNext",96],["musicMore",96]]) {
            const button=findChild(view,name);verify(button,name);
            compare(button.width,size,name);compare(button.height,size,name);compare(button.cornerRadius,size/2,name);
        }
    }
    // A single named as its album says it once (#840).
    function test_album_line_is_hidden_when_it_repeats_the_title() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const track={id:"t",title:"Fixture single",artist:"Fixture artist",album:"Fixture album",duration_us:60000000};
        catalog.activeState={session:"s",playback:"playing",current:{track:track},actions:["pause"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const album=findChild(view,"musicPlayerAlbum");verify(album);
        verify(album.visible);compare(album.text,"Fixture album");
        catalog.activeState={session:"s",playback:"playing",current:{track:Object.assign({},track,{album:"FIXTURE Single"})},actions:["pause"],upcoming:[]};
        tryCompare(album,"visible",false);
        catalog.activeState={session:"s",playback:"playing",current:{track:Object.assign({},track,{album:""})},actions:["pause"],upcoming:[]};
        tryCompare(album,"visible",false);
    }
    // Global navigation's Music entry lands on the room, not on the player
    // it was left in; the session keeps playing (#840, MH-1).
    function test_music_entry_lands_on_the_room() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);wait(0);
        view.openPlayer();wait(0);compare(view.mode,"player");
        view.openMore();wait(0);verify(view.moreOpen);
        view.enterSection();
        compare(view.mode,"root");compare(view.moreOpen,false);compare(view.overlayOpen,false);
        compare(catalog.commands,[]);compare(catalog.activeState.playback,"playing");
        tryVerify(()=>findChild(view,"musicNowPlaying").visible===false);
        // Without a room the catalogue stays where it is.
        const plain=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(plain);
        plain.openPlayer();wait(0);plain.enterSection();compare(plain.mode,"player");
    }
    function test_browsing_preserves_active_session_and_replacement_waits_for_confirmation() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        compare(catalog.commands.length,0);
        view.requestPlay("track-1");compare(view.pendingPlay,"track-1");compare(catalog.commands.length,0);
        const keep=findChild(view,"musicKeepQueue"),replace=findChild(view,"musicReplaceQueue");
        tryCompare(keep,"activeFocus",true);keyClick(Qt.Key_Right);tryCompare(replace,"activeFocus",true);
        keyClick(Qt.Key_Left);tryCompare(keep,"activeFocus",true);keyClick(Qt.Key_Return);
        compare(view.pendingPlay,"");compare(catalog.commands.length,0);
    }
    // A station entry (My Wave) opened as a folder of itself instead of
    // playing; a playlist still opens.
    function test_a_station_plays_and_a_playlist_opens() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"",playback:"stopped",actions:[],upcoming:[]};
        catalog.state={account:"connected",facets:["selections"],actions:["play","radio"]};
        catalog.entries=[{id:"station:wave",kind:"station",title:"Fixture wave",available:true},
                         {id:"playlist:1",kind:"playlist",title:"Fixture list",available:true}];
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);view.focusInitial();
        const entries=findChild(view,"musicEntries");tryCompare(entries,"activeFocus",true);
        keyClick(Qt.Key_Return);
        compare(catalog.commands,["radio"]);compare(catalog.played,["station:wave"]);
        const before=catalog.browsed.length;
        view.mode="catalogue";wait(0);entries.currentIndex=1;Ui.Focus.take(entries);wait(0);
        keyClick(Qt.Key_Return);
        compare(catalog.commands,["radio"]);compare(catalog.browsed.slice(before),["playlist:1"]);
    }
    function test_controls_B_fit_one_row_and_play_pause_uses_icon() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        view.openPlayer();wait(0);
        const controls=findChild(view,"musicControlsB"),button=findChild(view,"musicPlayPause");verify(controls);verify(button);
        // #802 (MH-3): previous 96, play/pause 120, next 96, the pills and
        // More in one vertically centred row; no Stop in the row.
        compare(findChild(view,"musicStop"),null);
        const previous=findChild(view,"musicPrevious"),next=findChild(view,"musicNext"),queue=findChild(view,"musicQueue"),more=findChild(view,"musicMore");
        // M14: the focus labels under the round buttons are 26.
        Ui.Focus.take(next);tryCompare(next,"activeFocus",true);
        const focusLabel=next.children.find(child=>child.text===next.text&&child.visible);verify(focusLabel);compare(focusLabel.font.pixelSize,26);
        compare(previous.width,96);compare(button.width,120);compare(next.width,96);compare(more.width,96);compare(more.iconName,"more_horiz");
        // Layout centres, before the focus lift.
        const centre=item=>item.parent.mapToItem(controls,0,item.y+item.height/2).y;
        for(const item of [previous,next,queue,more]) compare(centre(item),centre(button));
        compare(button.iconName,"pause");
        verify(more.mapToItem(view,more.width,0).x<=1920-Ui.Theme.safeRight);
        Ui.Focus.take(button);keyClick(Qt.Key_Right);
        tryCompare(next,"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(queue,"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(more,"activeFocus",true);
        keyClick(Qt.Key_Right);compare(more.activeFocus,true);
        keyClick(Qt.Key_Left);keyClick(Qt.Key_Left);keyClick(Qt.Key_Left);tryCompare(button,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"pause");
    }
    function test_home_transport_focus_survives_metadata_refresh() {
        const row=createTemporaryObject(rowFactory,suite);verify(row);wait(0);verify(row.focusIndex(0));
        keyClick(Qt.Key_Right);const button=findChild(row,"cardSecondaryAction");verify(button);tryCompare(button,"activeFocus",true);
        row.entries=[{id:"music",title:"Updated fixture",secondaryIcon:"pause",secondaryText:"Pause"}];wait(0);
        compare(button.activeFocus,true);keyClick(Qt.Key_Left);compare(findChild(row,"entry-music").activeFocus,true);
    }
    function test_account_and_empty_catalogue_actions_are_reachable_by_remote() {
        const catalog=createTemporaryObject(catalogFactory,suite);catalog.entries=[];
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        for(const item of [{account:"disconnected",action:"connect",button:"musicConnect"},{account:"connecting",action:"cancel",button:"musicCancelAuth"},{account:"connected",action:"disconnect",button:"musicDisconnect"},{account:"local",action:"scan",button:"musicRescan"}]) {
            catalog.state={account:item.account,facets:[],actions:[item.action]};wait(0);view.focusInitial();
            const button=findChild(view,item.button);tryCompare(button,"activeFocus",true);
            keyClick(Qt.Key_Up);keyClick(Qt.Key_Down);tryCompare(button,"activeFocus",true);
            keyClick(Qt.Key_Right);tryCompare(findChild(view,"musicSettings"),"activeFocus",true);
            keyClick(Qt.Key_Left);tryCompare(findChild(view,"musicOpenPlayer"),"activeFocus",true);
            keyClick(Qt.Key_Left);tryCompare(findChild(view,"musicKeyboard"),"activeFocus",true);
            keyClick(Qt.Key_Left);keyClick(Qt.Key_Down);tryCompare(button,"activeFocus",true);
        }
    }
    function test_load_more_and_search_keyboard_remote_paths() {
        const catalog=createTemporaryObject(catalogFactory,suite);catalog.more=true;
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);view.focusInitial();
        keyClick(Qt.Key_Down);tryCompare(findChild(view,"musicLoadMore"),"activeFocus",true);
        keyClick(Qt.Key_Up);wait(0);keyClick(Qt.Key_Up);tryCompare(findChild(view,"musicSearch"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.typing,true);keyClick(Qt.Key_Escape);compare(view.typing,false);
    }
    function test_account_transitions_keep_remote_focus_without_manual_restore() {
        const catalog=createTemporaryObject(catalogFactory,suite);catalog.entries=[];
        catalog.state={account:"disconnected",facets:[],actions:["connect"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);view.focusInitial();
        tryCompare(findChild(view,"musicConnect"),"activeFocus",true);keyClick(Qt.Key_Return);compare(catalog.commands[0],"connect");
        catalog.state={account:"connecting",facets:[],actions:["cancel"]};tryCompare(findChild(view,"musicCancelAuth"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[1],"cancel");
        catalog.state={account:"disconnected",facets:[],actions:["connect"]};tryCompare(findChild(view,"musicConnect"),"activeFocus",true);
        keyClick(Qt.Key_Return);catalog.state={account:"connecting",facets:[],actions:["cancel"]};tryCompare(findChild(view,"musicCancelAuth"),"activeFocus",true);
        catalog.state={account:"connected",facets:[],actions:["disconnect"]};tryCompare(findChild(view,"musicDisconnect"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[catalog.commands.length-1],"disconnect");
    }
    function test_transport_busy_receipt_restores_focused_button() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();
        const play=findChild(view,"musicPlayPause");tryCompare(play,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"pause");catalog.busy=true;wait(0);
        catalog.activeState=({session:"session",playback:"paused",current:catalog.activeState.current,actions:["resume","stop"],upcoming:[]});catalog.busy=false;
        tryCompare(play,"activeFocus",true);keyClick(Qt.Key_Return);compare(catalog.commands[1],"resume");
    }
    function test_music_navigation_keeps_system_actions_after_sections() {
        const home=createTemporaryObject(homeFactory,suite,{musicTitle:"Fixture music"});verify(home);
        compare(home.routes.length,6);compare(home.routes[2],"music");
        home.selectSection(2);compare(home.screen,"music");
        home.navOpen=true;
        // With a music section the system items move down by one: Open
        // services and Running stay visible.
        compare(findChild(home,"nav-6").text,"Open services");compare(findChild(home,"nav-6").visible,true);
        // #759 CL-7: Running replaces the activity row and stays.
        compare(findChild(home,"nav-7").text,"Running");compare(findChild(home,"nav-7").visible,true);
        home.returnActivityId="activity";compare(findChild(home,"nav-7").visible,true);
        home.currentServiceTitle="Fixture service";
        compare(home.navIcon(2),"music_note");compare(home.navIcon(3),"download");
        compare(home.navIcon(6),"public");compare(home.navIcon(8),"power");
        const items=[];
        for (let i=0;i<10;i++) items.push(findChild(home,"nav-"+i));
        for (let i=1;i<items.length;i++)
            verify(items[i-1].y+items[i-1].height<=items[i].y,"navigation items must not overlap with music enabled");
        Ui.Focus.take(items[6]);keyClick(Qt.Key_Return);compare(home.navOpen,false);home.navOpen=true;
        home.musicTitle="";compare(home.routes.length,5);compare(home.routes[2],"downloads");
        compare(findChild(home,"nav-5").visible,true);compare(findChild(home,"nav-6").visible,true);
    }
    // Review of #710: Up on the source picker must not switch the source;
    // a source changes only when its chip is activated.
    function test_source_picker_changes_source_only_when_activated() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.sources=[{id:"local",name:"Fixture music"},{id:"other",name:"Other music"}];
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        const picker=findChild(view,"musicSourcePicker");Ui.Focus.take(picker);
        keyClick(Qt.Key_Up);compare(catalog.sourceId,"local");
        keyClick(Qt.Key_Return);compare(view.pickingSource,true);wait(0);
        keyClick(Qt.Key_Down);compare(catalog.sourceId,"local");
        keyClick(Qt.Key_Return);compare(catalog.sourceId,"other");compare(view.pickingSource,false);
    }
    function test_remote_seek_moves_ten_seconds_per_press() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState=({session:"session",playback:"playing",position_us:30000000,current:{occurrence:"occurrence",track:{id:"track-0",title:"Existing track",duration_us:60000000}},actions:["pause","seek"],upcoming:[]});
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const seek=findChild(view,"musicSeek");verify(seek);Ui.Focus.take(seek);tryCompare(seek,"activeFocus",true);
        keyClick(Qt.Key_Right);compare(catalog.commands[0],"seek");compare(catalog.positions[0],40000000);
        keyClick(Qt.Key_Right);compare(catalog.positions[1],50000000);
        keyClick(Qt.Key_Right);keyClick(Qt.Key_Right);compare(catalog.positions[3],60000000);
        keyClick(Qt.Key_Left);compare(catalog.positions[4],50000000);
        keyClick(Qt.Key_Down);tryCompare(findChild(view,"musicPlayPause"),"activeFocus",true);
    }
    // Review of #710: OK on an upcoming entry asks before removing it.
    function test_queue_entry_removal_needs_confirmation() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState=({session:"session",playback:"playing",current:{occurrence:"occurrence",track:{id:"track-0",title:"Existing track",duration_us:10000000}},actions:["pause","remove"],upcoming:[{occurrence:"next-1",track:{id:"track-2",title:"Next track"}}]});
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        view.mode="queue";wait(0);view.focusInitial();
        tryCompare(findChild(view,"musicQueueEntries"),"activeFocus",true);
        // #802: the current entry comes first and is not removable.
        keyClick(Qt.Key_Return);compare(view.queueOccurrence,"");keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);compare(catalog.commands.length,0);
        const keep=findChild(view,"musicKeepOccurrence"),remove=findChild(view,"musicRemoveOccurrence");
        tryCompare(keep,"activeFocus",true);keyClick(Qt.Key_Return);compare(catalog.commands.length,0);
        tryCompare(findChild(view,"musicQueueEntries"),"activeFocus",true);
        keyClick(Qt.Key_Return);tryCompare(keep,"activeFocus",true);keyClick(Qt.Key_Right);tryCompare(remove,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"remove");compare(catalog.occurrences[0],"next-1");
    }
    // Review of #710: the Home card's Play/Pause was disabled while a command
    // was busy, so focus fell off it; a press while busy is ignored instead.
    function test_home_transport_keeps_focus_while_busy() {
        const row=createTemporaryObject(rowFactory,suite);verify(row);wait(0);verify(row.focusIndex(0));
        let presses=0;row.secondaryActivated.connect(()=>presses++);
        keyClick(Qt.Key_Right);const button=findChild(row,"cardSecondaryAction");tryCompare(button,"activeFocus",true);
        row.entries=[{id:"music",title:"Fixture",secondaryIcon:"pause",secondaryText:"Pause",busy:true}];wait(0);
        compare(button.activeFocus,true);keyClick(Qt.Key_Return);compare(presses,0);
        row.entries=[{id:"music",title:"Fixture",secondaryIcon:"play",secondaryText:"Play",busy:false}];wait(0);
        compare(button.activeFocus,true);keyClick(Qt.Key_Return);compare(presses,1);
    }
    function test_playlist_actions_have_remote_entry_paths() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["playlists"],actions:["play","save_playlist","delete_playlist"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,parentId:"playlist:fixture",parentKind:"playlist"});
        verify(view);wait(0);view.focusInitial();
        keyClick(Qt.Key_Down);tryCompare(findChild(view,"musicSavePlaylist"),"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(findChild(view,"musicDeletePlaylist"),"activeFocus",true);
        keyClick(Qt.Key_Left);keyClick(Qt.Key_Return);compare(view.namingPlaylist,true);
        const name=findChild(view,"musicPlaylistName");tryCompare(name,"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(findChild(view,"musicPlaylistKeyboard"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.typing,true);
        keyClick(Qt.Key_Escape);compare(view.typing,false);compare(view.namingPlaylist,true);
        keyClick(Qt.Key_Escape);compare(view.namingPlaylist,false);
    }
    function test_save_queue_has_remote_entry_path() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"session",playback:"paused",actions:["save_playlist"],upcoming:[{occurrence:"one",track:{id:"track-1",title:"Track"}}]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,mode:"queue"});
        verify(view);wait(0);view.focusInitial();keyClick(Qt.Key_Up);
        tryCompare(findChild(view,"musicSaveQueue"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.namingPlaylist,true);compare(view.playlistActive,true);
    }

    function test_initially_empty_radio_still_requires_replacement_confirmation() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"station-session",playback:"ended",continuous:true,actions:["stop"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        view.requestPlay("track-1");compare(view.pendingPlay,"track-1");compare(catalog.commands.length,0);
        tryCompare(findChild(view,"musicKeepQueue"),"activeFocus",true);
        keyClick(Qt.Key_Escape);view.openPlayer();wait(0);
        // #802: Stop is in More, which lists only what the session offers.
        const more=findChild(view,"musicMore");Ui.Focus.take(more);keyClick(Qt.Key_Return);compare(view.moreOpen,true);
        const stop=findChild(view,"musicMore-stop");tryCompare(stop,"activeFocus",true);
        compare(findChild(view,"musicMoreActions").count,1);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"stop");compare(view.moreOpen,false);
        tryCompare(more,"activeFocus",true);
    }

    function test_oauth_success_with_music_denied_is_not_an_empty_connected_library() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.entries=[];
        catalog.state={account:"restricted",backend:"unavailable",error:"PermissionDenied",facets:[],actions:["connect","disconnect"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        compare(findChild(view,"musicAccountHeading").text,"Music access was denied");
        compare(findChild(view,"musicAccountExplanation").text,"Sign-in succeeded, but this application cannot access music.");
        compare(findChild(view,"musicEmptyCatalogue").visible,false);
        view.focusInitial();tryCompare(findChild(view,"musicConnect"),"activeFocus",true);
    }

    function test_browser_confirmation_is_reachable_after_the_short_code_is_shown() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"connecting",facets:[],actions:["authorize","cancel"],prompt:{verification_url:"https://fixture.invalid/device",user_code:"FIXTURE",expires_in:300}};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.focusInitial();
        tryCompare(findChild(view,"musicAuthorize"),"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(findChild(view,"musicCancelAuth"),"activeFocus",true);
        keyClick(Qt.Key_Left);tryCompare(findChild(view,"musicAuthorize"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"authorize");
    }

    function covers(item,view) {
        const origin=item.mapToItem(view,0,0);
        return origin.x<=0 && origin.y<=0 && origin.x+item.width>=view.width && origin.y+item.height>=view.height;
    }
    function inside(item,view) {
        const origin=item.mapToItem(view,0,0);
        return item.width>0 && item.height>0 && origin.x>=0 && origin.y>=0
            && origin.x+item.width<=view.width && origin.y+item.height<=view.height;
    }
    // The shell gave the view no size: the replace question had no scrim
    // and no plate, and the keyboard panel opened above the screen (#756
    // M6, M7). The view fills the logical canvas itself.
    function test_replace_dialog_and_keyboard_show_in_the_shell() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(shellViewFactory,suite,{catalog:catalog});verify(view);wait(0);
        compare(view.width,1920);compare(view.height,1080);
        view.requestPlay("track-1");
        const dialog=findChild(view,"musicReplaceDialog");tryCompare(dialog,"visible",true);
        verify(covers(findChild(dialog,"musicDialogScrim"),view),"the scrim covers the screen");
        const plate=findChild(dialog,"musicDialogPlate");
        compare(plate.width,900);compare(plate.radius,48);
        const centre=plate.mapToItem(view,plate.width/2,plate.height/2);
        compare(Math.round(centre.x),960);compare(Math.round(centre.y),540);
        tryCompare(findChild(view,"musicKeepQueue"),"activeFocus",true);
        keyClick(Qt.Key_Escape);tryCompare(dialog,"visible",false);
        const toggle=findChild(view,"musicKeyboard");Ui.Focus.take(toggle);tryCompare(toggle,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.typing,true);
        const keyboard=findChild(view,"musicOnScreenKeyboard");
        tryVerify(()=>keyboard.activeFocus);
        verify(inside(keyboard,view),"the keyboard panel is on screen");
        keyClick(Qt.Key_Escape);compare(view.typing,false);tryCompare(findChild(view,"musicSearch"),"activeFocus",true);
    }
    // The bare "Unavailable" had no reason (#756 M5): the line shows the
    // catalogue's notice, and an unavailable row says why.
    function test_notice_and_unavailable_rows_carry_a_reason() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.entries=[{id:"t1",kind:"track",title:"Fixture",artist:"Fixture artist",available:false}];
        catalog.state={account:"local",facets:["tracks"],actions:["play"],error:"Unavailable"};
        catalog.activeState={session:"s",playback:"playing",error:"Unavailable",current:{track:{id:"t0",title:"Playing"}},actions:["pause"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        const notice=findChild(view,"musicNotice");
        // A stale provider error alone is not shown while music plays.
        compare(notice.visible,false);
        catalog.notice="Playback stopped. The track could not be played.";
        tryCompare(notice,"visible",true);compare(notice.text,catalog.notice);
        catalog.notice="";tryCompare(notice,"visible",false);
        const row=findChild(view,"musicEntries").currentItem;verify(row);
        let texts=[];
        (function walk(item){for(const child of item.children){if(child.text!==undefined && child.role!==undefined)texts.push(child.text);walk(child);}})(row);
        verify(texts.includes("Fixture artist · This source cannot play it"),texts.join("|"));
        verify(!texts.includes("Unavailable"));
    }
    // A failed player named the failure as its title and offered nothing
    // (#756 M8): the heading keeps the state, the cause and Retry follow.
    function test_failed_player_shows_the_cause_and_retry() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"stopped",error:"NotFound",current:{track:{id:"track-0",title:"Fixture"}},actions:["play"],upcoming:[]};
        catalog.playbackProblem="The track was not found in this source.";
        catalog.notice="Playback stopped. The track was not found in this source.";
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        compare(findChild(view,"musicPlayerContext").text,"Fixture music · stopped");
        compare(findChild(view,"musicPlaybackProblem").text,"Playback failed. The track was not found in this source.");
        compare(findChild(view,"musicNotice").visible,false);
        const retry=findChild(view,"musicRetryPlayback");verify(retry.visible);
        const play=findChild(view,"musicPlayPause");Ui.Focus.take(play);tryCompare(play,"activeFocus",true);
        keyClick(Qt.Key_Down);tryCompare(retry,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands,["play"]);
        keyClick(Qt.Key_Up);tryCompare(play,"activeFocus",true);
        // Review of #797: a station's `retry` only reloads its queue, so a
        // failed station plays its next loaded track; with none loaded the
        // failure offers no action of its own.
        catalog.activeState={session:"s",playback:"stopped",continuous:true,error:"Unavailable",current:{track:{id:"track-0",title:"Fixture"}},actions:["retry","next"],upcoming:[]};
        wait(0);compare(retry.visible,false);
        catalog.activeState={session:"s",playback:"stopped",continuous:true,error:"Unavailable",current:{track:{id:"track-0",title:"Fixture"}},actions:["retry","next"],upcoming:[{occurrence:"o2",track:{id:"track-1",title:"Next"}}]};
        tryCompare(retry,"visible",true);compare(retry.text,"Play next");
        Ui.Focus.take(retry);keyClick(Qt.Key_Return);compare(catalog.commands,["play","next"]);
        // Review of #797: the recovered player hid Retry with focus still on
        // it, and a second OK sent another retry.
        catalog.activeState={session:"s",playback:"playing",continuous:true,current:{track:{id:"track-0",title:"Fixture"}},actions:["pause","retry"],upcoming:[]};
        catalog.playbackProblem="";
        tryCompare(findChild(view,"musicPlaybackFailure"),"visible",false);
        tryCompare(play,"activeFocus",true);compare(retry.activeFocus,false);
        view.retryPlayback();compare(catalog.commands,["play","next"]);
    }
    // Saved positions pushed the playing music past the four further
    // cards of Continue (#756 M3).
    function test_continue_keeps_the_music_card_ahead_of_saved_positions() {
        const saved=[];
        for(let i=0;i<5;i++)saved.push({id:"saved-"+i,kind:"media",title:"Saved "+i,libraryEntryId:"l"+i});
        const home=createTemporaryObject(homeFactory,suite,{
            continueEntries:[{id:"activity",title:"Fixture application",running:true}].concat(saved)
                .concat([{id:"arinn-music-session",kind:"music",live:true,title:"Fixture song",subtitle:"Fixture artist"}])});
        verify(home);
        const ids=home.continuePresentation.map(entry=>entry.id);
        compare(ids[0],"activity");compare(ids[1],"arinn-music-session");compare(ids.length,5);
        // Review of #797: a stopped session does not displace saved
        // positions (spec §5.1).
        home.continueEntries=home.continueEntries.map(entry=>entry.kind==="music" ? Object.assign({},entry,{live:false}) : entry);
        compare(home.continuePresentation.map(entry=>entry.id).indexOf("arinn-music-session"),-1);
    }

    function iconOf(button) {
        for(const child of button.contentItem.children) if(child.name!==undefined) return child;
        return null;
    }
    // One icon set: the music buttons and the artwork placeholder draw
    // LineIcon glyphs (#756 M23); scrims and radii are theme tokens (M22).
    function test_music_uses_line_icons_and_theme_tokens() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T"}},actions:["pause","like","dislike","next","previous","seek","stop","radio","pin","retry"],upcoming:[],station:{id:"st",title:"Station"}};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const play=findChild(view,"musicPlayPause");const glyph=iconOf(play);
        verify(glyph);compare(glyph.name,"pause");
        verify(glyph.toString().indexOf("LineIcon")>=0,glyph.toString());
        compare(iconOf(findChild(view,"musicNext")).name,"skip_next");
        compare(findChild(findChild(view,"musicQueue"),"musicPillIcon").name,"queue_music");
        compare(iconOf(findChild(view,"musicMore")).name,"more_horiz");
        view.mode="catalogue";wait(0);
        const plate=findChild(view,"musicSearch").parent;compare(plate.radius,Ui.Theme.containerRadius);
        view.requestPlay("track-1");
        compare(findChild(findChild(view,"musicReplaceDialog"),"musicDialogScrim").color,Ui.Theme.scrim);
        compare(findChild(findChild(view,"musicReplaceDialog"),"musicDialogPlate").radius,Ui.Theme.dialogRadius);
        const art=createTemporaryObject(artworkFactory,suite);compare(art.radius,Ui.Theme.stillRadius);
    }
    // An album, artist or playlist title is content: Playfair Display with
    // lining figures (ADR-0137,
    // #756 M24). The section title stays Onest.
    function test_detail_heading_is_a_content_title() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        const heading=findChild(view,"musicHeading");
        compare(heading.font.family,Ui.Theme.fontFamily);
        view.parentId="album:1";view.parentTitle="Fixture album";
        compare(heading.text,"Fixture album");compare(heading.font.family,Ui.Theme.contentFontFamily);
        compare(heading.font.features.lnum,1);
    }
    // Hours in long tracks, no raw facet ids, labels that set their widths
    // (#756 M25).
    function test_time_facets_and_action_widths() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["tracks","mood","albums"],actions:["play"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        compare(view.timeLabel(185000000),"3:05");compare(view.timeLabel(4503000000),"1:15:03");
        compare(view.timeLabel(3600000000),"1:00:00");
        const labels=[];for(let i=0;;i++){const button=findChild(view,"musicFacet-"+i);if(!button)break;labels.push(button.text);}
        compare(labels,["Tracks","Albums"]);
        const open=findChild(view,"musicOpenPlayer"),settings=findChild(view,"musicSettings");
        open.text="A much longer translated label for Open player";wait(0);
        verify(open.width>=open.implicitWidth);
        const plate=findChild(view,"musicSearch").parent;
        verify(plate.x+plate.width<=open.mapToItem(view,0,0).x,"the search field leaves room for the actions");
        verify(settings.mapToItem(view,settings.width,0).x<=1920-Ui.Theme.safeRight,"the actions stay in the safe area");
    }
    // Play/Pause is the player's first focus; while it waits for a command
    // a working control holds focus and hands it back (#756 M13).
    function test_play_pause_is_the_default_focus() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const playing={session:"s",playback:"playing",current:{track:{id:"t",title:"T",duration_us:60000000}},actions:["pause","stop","next","seek"],upcoming:[]};
        const loading=Object.assign({},playing,{playback:"loading",actions:["stop","next","seek"]});
        catalog.activeState=playing;
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        view.openPlayer();const play=findChild(view,"musicPlayPause");tryCompare(play,"activeFocus",true);
        // Review of #798: a command in flight keeps Play/Pause and its
        // focus; a press meanwhile sends nothing.
        catalog.busy=true;wait(0);compare(play.activeFocus,true);compare(play.enabled,true);
        keyClick(Qt.Key_Return);compare(catalog.commands,[]);
        catalog.busy=false;
        // A track loading disables it under focus: Next holds focus and
        // hands it back once Play/Pause works again (Stop is in More, #802).
        const stop=findChild(view,"musicNext");
        catalog.activeState=loading;tryCompare(stop,"activeFocus",true);
        catalog.activeState=playing;tryCompare(play,"activeFocus",true);
        // Opened while loading, the same.
        view.mode="catalogue";catalog.activeState=loading;wait(0);view.openPlayer();
        tryCompare(stop,"activeFocus",true);
        catalog.activeState=playing;tryCompare(play,"activeFocus",true);
        // A control the user chose keeps focus.
        catalog.activeState=loading;tryCompare(stop,"activeFocus",true);keyClick(Qt.Key_Up);
        const seek=findChild(view,"musicSeek");tryCompare(seek,"activeFocus",true);
        catalog.activeState=playing;wait(50);compare(seek.activeFocus,true);
    }
    // Down no longer moves sideways inside the one control row, and every
    // control reaches the seek bar with Up (#756 M15).
    function test_player_dpad_graph() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T",duration_us:60000000}},actions:["pause","like","dislike","next","previous","seek","stop","radio"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const play=findChild(view,"musicPlayPause"),seek=findChild(view,"musicSeek");
        tryCompare(play,"activeFocus",true);
        for(const name of ["musicPrevious","musicPlayPause","musicNext","musicLike","musicQueue","musicMore"]) {
            const button=findChild(view,name);verify(button,name);Ui.Focus.take(button);tryCompare(button,"activeFocus",true);
            keyClick(Qt.Key_Down);compare(button.activeFocus,true,name+": Down stays in place");
            keyClick(Qt.Key_Up);tryCompare(seek,"activeFocus",true);
        }
        // #802 (M15): Down from the seek line returns to play/pause, and
        // Left and Right stop at the row's ends.
        keyClick(Qt.Key_Down);tryCompare(play,"activeFocus",true);
        const previous=findChild(view,"musicPrevious"),more=findChild(view,"musicMore");
        Ui.Focus.take(previous);keyClick(Qt.Key_Left);compare(previous.activeFocus,true);
        Ui.Focus.take(more);keyClick(Qt.Key_Right);compare(more.activeFocus,true);
        for(const name of ["musicDislike","musicStop","musicRadio","musicSeekButton"]) compare(findChild(view,name),null,name);
    }
    // #802 (brief mh-03, M18): Like shows a filled heart and "Liked", with
    // pending and failure states, and says so to accessibility.
    function test_like_shows_its_state() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T",duration_us:60000000}},actions:["pause","like"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const like=findChild(view,"musicLike"),icon=findChild(like,"musicPillIcon");
        compare(like.text,"Like");compare(icon.name,"favorite");compare(like.Accessible.checked,false);compare(like.Accessible.checkable,true);
        // Codex review of #821: another command in flight makes Like busy
        // too, and says so.
        catalog.busy=true;tryCompare(like,"busy",true);compare(like.Accessible.description,"Another command is in progress.");
        catalog.busy=false;tryCompare(like,"busy",false);compare(like.Accessible.description,"");
        catalog.busyOnTransport=true;
        Ui.Focus.take(like);keyClick(Qt.Key_Return);
        compare(catalog.commands,["like"]);tryCompare(like,"text","Saving…");compare(like.Accessible.description,"Saving…");
        // The provider refuses it.
        catalog.errorText="The source could not do that. Try again.";catalog.busy=false;
        tryCompare(like,"text","Not saved");verify(like.Accessible.description.length>0);
        // Codex review of #821: another command in flight is said before the
        // older failure.
        catalog.busy=true;tryCompare(like.Accessible,"description","Another command is in progress.");
        catalog.busy=false;tryCompare(like.Accessible,"description","The like was not saved. Try again.");
        // A confirmed like.
        catalog.errorText="";keyClick(Qt.Key_Return);tryCompare(like,"text","Saving…");
        catalog.activeState=Object.assign({},catalog.activeState,{feedback:"liked"});catalog.busy=false;
        tryCompare(like,"text","Liked");compare(icon.name,"favorite_fill");compare(like.Accessible.checked,true);compare(like.selected,true);
        // Review of #821: busy ends before the next state brings the
        // feedback; the label stays "Saving…" until it does, never "Like".
        const labels=[];like.textChanged.connect(()=>labels.push(like.text));
        keyClick(Qt.Key_Return);tryCompare(like,"text","Saving…");
        catalog.busy=false;wait(50);compare(like.text,"Saving…");
        // Codex review of #821: a stale state with the old feedback (a poll
        // already in flight) keeps it pending; the changed one ends it.
        catalog.activeState=Object.assign({},catalog.activeState,{position_us:1000000});wait(50);compare(like.text,"Saving…");
        catalog.activeState=Object.assign({},catalog.activeState,{feedback:""});
        tryCompare(like,"text","Like");compare(labels,["Saving…","Like"]);
        // A provider that keeps its feedback ends the wait on the third state.
        keyClick(Qt.Key_Return);tryCompare(like,"text","Saving…");catalog.busy=false;
        for(let i=0;i<2;i++){catalog.activeState=Object.assign({},catalog.activeState,{position_us:2000000+i});wait(50);compare(like.text,"Saving…");}
        catalog.activeState=Object.assign({},catalog.activeState,{position_us:3000000});tryCompare(like,"text","Like");
        // Codex review of #821: the track changes while a like is in flight;
        // its late failure does not mark the new track.
        keyClick(Qt.Key_Return);tryCompare(like,"text","Saving…");
        catalog.activeState=Object.assign({},catalog.activeState,{current:{track:{id:"t2",title:"T2",duration_us:60000000}}});
        tryCompare(like,"text","Like");
        catalog.errorText="The source could not do that. Try again.";catalog.busy=false;wait(50);
        compare(like.text,"Like");catalog.errorText="";
        // Review of #821: one width for every state, so Queue and More stay
        // put, and in Russian the row stays in the safe area.
        // The layout settles after each change before it is read (delta
        // review of #821).
        const queue=findChild(view,"musicQueue"),more=findChild(view,"musicMore");
        const settle=()=>{waitForRendering(view);wait(50);};
        settle();const at=more.mapToItem(view,0,0).x,width=like.width;
        for(const text of ["Liked","Saving…","Not saved","Like"]) {like.text=text;settle();compare(like.width,width,text);compare(more.mapToItem(view,0,0).x,at,text);}
        like.labels=["Нравится","Понравилось","Сохраняем…","Не сохранено"];queue.text="Очередь";like.text="Нравится";settle();
        const russian=like.width;
        for(const text of like.labels) {like.text=text;settle();compare(like.width,russian,text);verify(more.mapToItem(view,more.width,0).x<=1920-Ui.Theme.safeRight,text+": "+more.mapToItem(view,more.width,0).x);}
    }
    // #802 (brief *More sheet*): More lists only what the session supports,
    // in the brief's order; Back closes it and restores focus to More; the
    // Menu key never opens it.
    function test_more_sheet_lists_supported_actions() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T"}},station:{id:"station:wave",title:"Fixture wave"},continuous:true,
            actions:["pause","dislike","radio","pin","retry","stop"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const more=findChild(view,"musicMore");Ui.Focus.take(more);
        keyClick(Qt.Key_Menu);compare(view.moreOpen,false);
        keyClick(Qt.Key_Return);compare(view.moreOpen,true);verify(view.overlayOpen);
        const list=findChild(view,"musicMoreActions");tryCompare(list,"activeFocus",true);
        const ids=[];for(let i=0;i<list.count;i++)ids.push(list.itemAtIndex(i).actionKey);
        compare(ids,["dislike","radio","pin","retry","stop"]);
        compare(list.itemAtIndex(0).text,"Dislike");compare(list.itemAtIndex(2).text,"Pin station");
        // A state poll keeps the focused row.
        keyClick(Qt.Key_Down);keyClick(Qt.Key_Down);compare(list.currentIndex,2);
        catalog.activeState=Object.assign({},catalog.activeState,{position_us:1000000});wait(0);
        compare(list.currentIndex,2);verify(list.itemAtIndex(2).activeFocus);
        keyClick(Qt.Key_Left);keyClick(Qt.Key_Right);verify(list.itemAtIndex(2).activeFocus);
        keyClick(Qt.Key_Escape);compare(view.moreOpen,false);tryCompare(more,"activeFocus",true);
        // Pin sends the station, and the sheet closes.
        keyClick(Qt.Key_Return);tryCompare(list,"activeFocus",true);keyClick(Qt.Key_Down);keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Return);compare(catalog.commands,["pin"]);compare(view.moreOpen,false);tryCompare(more,"activeFocus",true);
        // Radio from this track asks before replacing the queue.
        keyClick(Qt.Key_Return);tryCompare(list,"activeFocus",true);keyClick(Qt.Key_Down);keyClick(Qt.Key_Return);
        compare(view.moreOpen,false);compare(view.pendingPlay,"t");
        // Review of #821: cancelling returns to More, by Back or by Cancel.
        keyClick(Qt.Key_Escape);compare(view.pendingPlay,"");tryCompare(more,"activeFocus",true);
        keyClick(Qt.Key_Return);tryCompare(list,"activeFocus",true);keyClick(Qt.Key_Down);keyClick(Qt.Key_Return);
        tryCompare(findChild(view,"musicKeepQueue"),"activeFocus",true);keyClick(Qt.Key_Return);
        compare(view.pendingPlay,"");tryCompare(more,"activeFocus",true);
        // Codex review of #821: a command in flight blocks every row, visibly,
        // Radio too: its confirmation would send the command at once.
        catalog.busyOnTransport=false;catalog.busy=true;
        Ui.Focus.take(more);keyClick(Qt.Key_Return);tryCompare(list,"activeFocus",true);
        const stopRow=findChild(view,"musicMore-stop");verify(stopRow.busy);verify(findChild(view,"musicMore-radio").busy);
        compare(stopRow.Accessible.description,"Loading");
        list.currentIndex=4;keyClick(Qt.Key_Return);compare(catalog.commands,["pin"]);compare(view.moreOpen,true);
        catalog.busy=false;tryCompare(stopRow,"busy",false);
        keyClick(Qt.Key_Escape);tryCompare(more,"activeFocus",true);
        // With nothing more to offer, the sheet says so.
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T"}},actions:["pause"],upcoming:[]};
        Ui.Focus.take(more);keyClick(Qt.Key_Return);
        tryCompare(findChild(view,"musicMoreEmpty"),"visible",true);compare(list.count,0);
        keyClick(Qt.Key_Escape);compare(view.moreOpen,false);tryCompare(more,"activeFocus",true);
    }
    // #802 (brief *Queue*): the current entry first, with the playing mark
    // and the initial focus; the station names the sheet; a continuous queue
    // ends with a closing line, never a total; an empty queue says so.
    function test_queue_sheet() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",continuous:true,station:{id:"station:wave",title:"Fixture wave"},
            current:{occurrence:"c",track:{id:"t0",title:"Now",artist:"Fixture artist",duration_us:185000000}},actions:["pause"],
            upcoming:[{occurrence:"u1",track:{id:"t1",title:"Next one",artist:"Other",duration_us:60000000}},{occurrence:"u2",track:{id:"t2",title:"Next two"}}]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const queue=findChild(view,"musicQueue");Ui.Focus.take(queue);keyClick(Qt.Key_Return);compare(view.mode,"queue");verify(view.overlayOpen);
        const list=findChild(view,"musicQueueEntries");tryCompare(list,"activeFocus",true);
        compare(list.currentIndex,0);compare(list.currentItem.objectName,"musicQueueCurrent");
        verify(findChild(list.currentItem,"musicQueuePlaying").visible);
        compare(findChild(view,"musicQueueTitle").text,"Fixture wave");
        const footer=findChild(view,"musicQueueFooter");verify(footer.visible);compare(footer.text,"More tracks will follow");
        compare(findChild(list.itemAtIndex(1),"musicQueueDuration").text,"1:00");
        // Codex review of #821: the current row keeps its duration too, with
        // the playing mark before it.
        const nowDuration=findChild(list.currentItem,"musicQueueDuration"),mark=findChild(list.currentItem,"musicQueuePlaying");
        verify(nowDuration.visible);compare(nowDuration.text,"3:05");
        verify(mark.mapToItem(list.currentItem,mark.width,0).x<=nowDuration.mapToItem(list.currentItem,0,0).x);
        // A state poll keeps the focused row.
        keyClick(Qt.Key_Down);compare(list.currentIndex,1);
        catalog.activeState=Object.assign({},catalog.activeState,{position_us:2000000});wait(0);
        compare(list.currentIndex,1);verify(list.currentItem.activeFocus);
        keyClick(Qt.Key_Escape);compare(view.mode,"player");tryCompare(queue,"activeFocus",true);
        // A finite queue has no closing line; an empty one says so.
        catalog.activeState={session:"s",playback:"stopped",actions:[],upcoming:[]};
        keyClick(Qt.Key_Return);compare(view.mode,"queue");
        tryCompare(findChild(view,"musicQueueEmpty"),"visible",true);compare(list.count,0);
        compare(findChild(view,"musicQueueEmpty").text,"The queue is empty");
        // Codex review of #821: with Save disabled by a command in flight,
        // focus stays in the sheet.
        catalog.activeState={session:"s",playback:"stopped",actions:["save_playlist"],upcoming:[]};catalog.busy=true;
        Ui.Focus.take(queue);tryCompare(queue,"activeFocus",true);
        view.focusInitial();const sheet=findChild(view,"musicQueueView");
        tryVerify(()=>sheet.activeFocus);compare(findChild(view,"musicSaveQueue").enabled,false);
        catalog.busy=false;
        keyClick(Qt.Key_Escape);compare(view.mode,"player");tryCompare(queue,"activeFocus",true);
    }
    // #802 (brief mh-03): the artwork is 560 with the light in its hue, the
    // eyebrow names the context, and the seek knob shows only in focus.
    function test_now_playing_copy_and_seek_line() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",position_us:30000000,station:{id:"station:wave",title:"Fixture wave"},
            current:{track:{id:"t",title:"T",artist:"Fixture artist",album:"Fixture album",duration_us:60000000}},actions:["pause","seek"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);view.openPlayer();wait(0);
        const art=findChild(view,"musicPlayerArtwork");compare(art.width,560);compare(art.height,560);
        const glow=findChild(view,"musicPlayerGlow");verify(glow);verify(glow.visible);
        // Review of #821: the light sits under the heading, not over it.
        const layers=glow.parent.children;verify(layers.indexOf(glow)<layers.indexOf(findChild(view,"musicHeading")));
        compare(findChild(view,"musicPlayerContext").text,"Fixture wave · Fixture music");
        compare(findChild(view,"musicPlayerArtist").font.pixelSize,32);compare(findChild(view,"musicPlayerAlbum").font.pixelSize,28);
        compare(findChild(view,"musicTrackTitle").font.family,Ui.Theme.contentFontFamily);
        const seek=findChild(view,"musicSeek"),knob=findChild(view,"musicSeekKnob"),fill=findChild(view,"musicSeekFill");
        compare(fill.color,Ui.Theme.ember);compare(knob.visible,false);
        Ui.Focus.take(seek);tryCompare(knob,"visible",true);
        keyClick(Qt.Key_Down);tryCompare(findChild(view,"musicPlayPause"),"activeFocus",true);compare(knob.visible,false);
        catalog.activeState=Object.assign({},catalog.activeState,{station:null});
        tryCompare(findChild(view,"musicPlayerContext"),"text","Album · Fixture music");
    }
    // Back restores focus: the queue to Queue, a detail to the entity it
    // was opened from, a player opened from Home to Home (#756 M16).
    function test_back_restores_focus() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["albums"],actions:["play"]};
        const albums=[{id:"album:1",kind:"album",title:"One",available:true},{id:"album:2",kind:"album",title:"Two",available:true},{id:"album:3",kind:"album",title:"Three",available:true}];
        catalog.entries=albums;
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);view.focusInitial();
        const list=findChild(view,"musicEntries");tryCompare(list,"activeFocus",true);
        keyClick(Qt.Key_Down);keyClick(Qt.Key_Down);compare(list.currentIndex,2);
        keyClick(Qt.Key_Return);compare(view.parentId,"album:3");
        catalog.entries=[{id:"t1",kind:"track",title:"Inside",available:true}];wait(0);
        keyClick(Qt.Key_Escape);compare(view.parentId,"");
        // As the catalogue does: the list clears while the page is in flight.
        catalog.busy=true;catalog.entries=[];wait(0);catalog.busy=false;catalog.entries=albums;
        tryCompare(list,"currentIndex",2);tryCompare(list,"activeFocus",true);
        view.openPlayer();wait(0);const queue=findChild(view,"musicQueue");Ui.Focus.take(queue);keyClick(Qt.Key_Return);
        compare(view.mode,"queue");keyClick(Qt.Key_Escape);compare(view.mode,"player");tryCompare(queue,"activeFocus",true);
        let backs=0;view.backRequested.connect(()=>backs++);
        keyClick(Qt.Key_Escape);compare(view.mode,"catalogue");compare(backs,0);
        view.openPlayerFromHome();wait(0);keyClick(Qt.Key_Escape);compare(backs,1);compare(view.mode,"catalogue");
    }
    // Review of #798: an entity from a later page was not found on the
    // first page after Back, and focus landed on an unrelated row.
    // Review of #798: status changes while a page loaded each spent one of
    // the ten loads, so an opener a few pages deep was never reached.
    function test_a_slow_page_does_not_spend_the_opener_budget() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["albums"],actions:["play"]};catalog.busyOnLoad=true;
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,parentId:"album:1050",parentKind:"album",parentTitle:"Deep"});
        verify(view);wait(0);
        const page=from=>{const rows=[];for(let i=from;i<from+100;i++)rows.push({id:"album:"+i,kind:"album",title:"A"+i,available:true});return rows;};
        view.back();catalog.more=true;
        let shown=page(0);catalog.entries=shown;
        // The opener is on the tenth further page, the last the budget allows.
        for(let p=1;p<=10;p++) {
            tryCompare(catalog,"loads",p);compare(catalog.busy,true);
            // Status polls while the page loads.
            for(let i=0;i<3;i++)view.restoreOpener();
            compare(catalog.loads,p);compare(view.restorePages,p);
            shown=shown.concat(page(p*100));catalog.more=p<10;catalog.entries=shown;catalog.busy=false;
        }
        const list=findChild(view,"musicEntries");
        tryCompare(list,"currentIndex",1050);tryCompare(list,"activeFocus",true);
        compare(catalog.loads,10);compare(view.restoreEntity,"");
    }
    function test_back_finds_an_opener_on_a_later_page() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["albums"],actions:["play"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,parentId:"album:150",parentKind:"album",parentTitle:"Late"});
        verify(view);wait(0);
        view.back();compare(view.restoreEntity,"album:150");
        const page=from=>{const rows=[];for(let i=from;i<from+100;i++)rows.push({id:"album:"+i,kind:"album",title:"A"+i,available:true});return rows;};
        catalog.more=true;catalog.entries=page(0);
        tryCompare(catalog,"loads",1);compare(view.restoreEntity,"album:150");
        catalog.more=false;catalog.entries=page(0).concat(page(100));
        const list=findChild(view,"musicEntries");
        tryCompare(list,"currentIndex",150);tryCompare(list,"activeFocus",true);compare(view.restoreEntity,"");
        // Review of #798: an interrupted search left its page count behind
        // and cut the next search short. Each Back starts a full budget.
        view.parentId="album:150";view.back();catalog.more=true;catalog.entries=page(0);
        tryCompare(catalog,"loads",2);view.visible=false;view.visible=true;
        compare(view.restorePages,0);
        view.restorePages=view.restorePageLimit;view.parentId="album:150";view.back();
        compare(view.restorePages,0);
    }
    // Review of #798: replacing the queue from a player opened on Home
    // reopened it as an ordinary player, so Back went to the catalogue.
    function test_replacement_keeps_the_home_origin() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.activeState={session:"s",playback:"playing",current:{track:{id:"t",title:"T"}},actions:["pause","radio"],upcoming:[]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        let backs=0;view.backRequested.connect(()=>backs++);
        view.openPlayerFromHome();wait(0);
        view.requestRadio("t",true);wait(0);const confirm=findChild(view,"musicReplaceQueue");
        Ui.Focus.take(confirm);tryCompare(confirm,"activeFocus",true);keyClick(Qt.Key_Return);
        compare(catalog.commands,["radio"]);compare(view.mode,"player");
        wait(0);keyClick(Qt.Key_Escape);compare(backs,1);
    }
    // Review of #798: the opener search stayed armed after the user asked
    // for another list or a page failed, then paged that list and pulled
    // focus off the control the user had moved to.
    function test_another_browse_or_a_failed_page_ends_the_opener_search() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        catalog.state={account:"local",facets:["albums","playlists"],actions:["play"]};
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,parentId:"album:150",parentKind:"album",parentTitle:"Late"});
        verify(view);wait(0);
        view.back();compare(view.restoreEntity,"album:150");
        const facet=findChild(view,"musicFacet-1");Ui.Focus.take(facet);tryCompare(facet,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.restoreEntity,"");compare(view.restorePages,0);
        catalog.more=true;catalog.entries=[{id:"playlist:1",kind:"playlist",title:"P",available:true}];wait(50);
        compare(catalog.loads,0);compare(facet.activeFocus,true);
        // A failed page ends it too.
        view.parentId="album:150";view.back();compare(view.restoreEntity,"album:150");
        // Review of #798: a failed command sets the same error line and
        // leaves it; only a failed page ends it.
        catalog.errorText="This action is not available.";wait(0);compare(view.restoreEntity,"album:150");
        catalog.errorText="";catalog.pageFailed();compare(view.restoreEntity,"");
        // Focus elsewhere in the view stays there when the opener's page arrives.
        view.parentId="album:2";view.back();Ui.Focus.take(facet);tryCompare(facet,"activeFocus",true);
        catalog.more=false;catalog.entries=[{id:"album:1",kind:"album",title:"A",available:true},{id:"album:2",kind:"album",title:"B",available:true}];
        const list=findChild(view,"musicEntries");tryCompare(list,"currentIndex",1);wait(50);
        compare(facet.activeFocus,true);compare(view.restoreEntity,"");
        // Review of #798: an empty final page ends it as well.
        view.parentId="album:9";view.back();compare(view.restoreEntity,"album:9");
        catalog.more=false;catalog.entries=[];tryCompare(view,"restoreEntity","");
        // Review of #798: an empty last page that lands while a command is
        // still busy ends the search once the command ends.
        catalog.entries=[{id:"album:1",kind:"album",title:"A",available:true}];wait(0);
        view.parentId="album:9";view.back();catalog.busy=true;catalog.entries=[];wait(50);
        compare(view.restoreEntity,"album:9");
        catalog.busy=false;tryCompare(view,"restoreEntity","");
    }
    // Review of #798: the loading hand-off and the opener restore pulled
    // focus from under a scope that held it (the Quick menu, power).
    function test_focus_hand_offs_leave_another_scope_alone() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const playing={session:"s",playback:"playing",current:{track:{id:"t",title:"T"}},actions:["pause","stop","next"],upcoming:[]};
        catalog.activeState=playing;
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);
        const scope=createTemporaryObject(scopeFactory,suite);const held=findChild(scope,"scopeItem");
        view.openPlayer();const play=findChild(view,"musicPlayPause");tryCompare(play,"activeFocus",true);
        scope.forceActiveFocus();tryCompare(held,"activeFocus",true);
        catalog.activeState=Object.assign({},playing,{playback:"loading",actions:["stop","next"]});wait(50);
        compare(held.activeFocus,true);compare(view.activeFocus,false);
        catalog.activeState=playing;view.mode="catalogue";
        catalog.state={account:"local",facets:["albums"],actions:["play"]};
        view.parentId="album:2";view.back();compare(view.restoreEntity,"album:2");
        scope.forceActiveFocus();tryCompare(held,"activeFocus",true);
        catalog.more=false;catalog.entries=[{id:"album:1",kind:"album",title:"A",available:true},{id:"album:2",kind:"album",title:"B",available:true}];
        const list=findChild(view,"musicEntries");tryCompare(list,"currentIndex",1);wait(50);
        compare(held.activeFocus,true);compare(view.activeFocus,false);
    }
    // #800: with a room the section opens on it. The hero describes the
    // focused entry, shelves keep the room's order with a source mark per
    // card, and the light follows the focused artwork.
    function test_room_opens_on_shelves_with_a_hero() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        compare(view.mode,"root");view.forceActiveFocus();
        const roomView=findChild(view,"musicRoom");verify(roomView.visible);roomView.focusInitial();
        const stations=findChild(view,"musicShelf-stations");tryCompare(stations,"activeFocus",true);
        compare(findChild(view,"musicHeroTitle").text,"Fixture mix");
        compare(findChild(view,"musicHeroEyebrow").text,"Station");
        compare(findChild(view,"musicHeroMeta").text,"Fixture service");
        // The catalogue's own source switch and housekeeping are gone (MH-5).
        compare(findChild(view,"musicSourcePicker").visible,false);
        compare(findChild(view,"musicDisconnect").visible,false);compare(findChild(view,"musicRescan").visible,false);
        keyClick(Qt.Key_Down);const albums=findChild(view,"musicShelf-albums");tryCompare(albums,"activeFocus",true);
        compare(findChild(view,"musicHeroTitle").text,"First");compare(findChild(view,"musicHeroMeta").text,"This computer");
        compare(albums.shown.map(entry=>entry.key),["local\nalbum:1","account\nalbum:1","local\nalbum:2"]);
        // Up from the first shelf reaches the header pills.
        keyClick(Qt.Key_Up);keyClick(Qt.Key_Up);tryCompare(findChild(view,"musicSources"),"activeFocus",true);
    }
    // #838 (TV check): the focused first card of a shelf stays whole at the
    // safe area, cards scrolled left fade at the screen edge, rows scrolled
    // up fade under the hero, a focused card has the mat and rim, and the
    // focused All tile keeps a light label on its dark plate.
    function test_room_edges_fades_and_focus() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const many=kind=>{const out=[];for(let i=0;i<12;i++)out.push(room.card("local",kind+":"+i,kind,kind+" "+i));return out;};
        room.shelves=[room.shelves[0],{id:"albums",title:"Albums",kind:"album",entries:many("album")},
                      {id:"forYou",title:"For you",kind:"playlist",entries:many("playlist")},
                      {id:"artists",title:"Artists",kind:"artist",entries:many("artist")},
                      {id:"recent",title:"Recent tracks",kind:"track",entries:many("track")}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();const roomView=findChild(view,"musicRoom");roomView.focusInitial();
        tryCompare(findChild(view,"musicShelf-stations"),"activeFocus",true);
        // Nothing scrolled yet: no top fade.
        verify(!findChild(view,"musicShelvesTopFade").visible);
        keyClick(Qt.Key_Down);const albums=findChild(view,"musicShelf-albums");tryCompare(albums,"activeFocus",true);
        const area=findChild(view,"musicShelves");
        const first=albums.focusedCard;verify(first);const card=findChild(first,"musicShelfCard");
        tryCompare(card,"scale",Ui.Theme.contentFocusScale);
        // The card, its mat and rim and its caption lie right of the clip.
        const mat=findChild(card,"focusMat");tryCompare(mat,"opacity",1);
        const clipLeft=area.mapToItem(view,0,0).x;
        verify(mat.mapToItem(view,0,0).x-Ui.Theme.rimThickness>=clipLeft,"mat "+mat.mapToItem(view,0,0).x+" clip "+clipLeft);
        verify(findChild(card,"musicCardTitle").mapToItem(view,0,0).x>=clipLeft);
        verify(card.mapToItem(view,0,0).x>=Ui.Theme.edgeFadeEnd,"left edge "+card.mapToItem(view,0,0).x);
        compare(findChild(card,"musicCardFocus").shown,true);
        // The title and the first card start at the safe area.
        compare(findChild(albums,"musicShelfTitle").mapToItem(view,0,0).x,Ui.Theme.safeLeft);
        const edge=findChild(albums,"musicShelfEdgeFade");verify(!edge.visible);
        for(let i=0;i<8;i++)keyClick(Qt.Key_Right);
        // Opaque from the screen edge to x 24, then fading out to x 112
        // (Codex review of #848).
        tryVerify(()=>edge.visible);compare(edge.mapToItem(view,0,0).x,0);compare(edge.width,Ui.Theme.edgeFadeEnd);
        const stops=edge.gradient.stops;compare(stops.length,3);
        compare(stops[1].position,Ui.Theme.edgeFadeStart/Ui.Theme.edgeFadeEnd);compare(stops[1].color.a,1);compare(stops[2].color.a,0);
        // Review of #848: a scrolled focused card stops right of the fade,
        // its mat and rim included.
        const scrolled=findChild(albums.focusedCard,"musicShelfCard");tryCompare(scrolled,"scale",Ui.Theme.contentFocusScale);
        const scrolledMat=findChild(scrolled,"focusMat");
        tryVerify(()=>scrolledMat.mapToItem(view,0,0).x-Ui.Theme.rimThickness>=Ui.Theme.edgeFadeEnd);
        // Rows scrolled up fade under the hero; the focused shelf's title
        // stays below the band.
        const top=findChild(view,"musicShelvesTopFade");
        keyClick(Qt.Key_Down);keyClick(Qt.Key_Down);keyClick(Qt.Key_Down);
        const recent=findChild(view,"musicShelf-recent");tryCompare(recent,"activeFocus",true);
        tryVerify(()=>area.contentY>0);tryVerify(()=>top.visible);compare(top.mapToItem(view,0,0).y,area.mapToItem(view,0,0).y);
        tryVerify(()=>findChild(recent,"musicShelfTitle").mapToItem(view,0,0).y>=top.mapToItem(view,0,0).y+top.height-1);
        // The All tile in focus: a dark container plate, a light label.
        for(let i=0;i<12;i++)keyClick(Qt.Key_Right);
        const all=findChild(recent.focusedCard,"musicShelfAll");tryCompare(all,"activeFocus",true);
        const label=findChild(all,"musicShelfAllLabel");
        const near=(a,b)=>["r","g","b","a"].every(k=>Math.abs(a[k]-b[k])<=1/255);verify(near(label.color,Ui.Theme.textPrimary));
        const lum=c=>{const f=v=>v<=0.03928?v/12.92:Math.pow((v+0.055)/1.055,2.4);return 0.2126*f(c.r)+0.7152*f(c.g)+0.0722*f(c.b);};
        const plate=all.background.color;const a=lum(label.color),b=lum(plate);
        verify((Math.max(a,b)+0.05)/(Math.min(a,b)+0.05)>=4.5,"contrast "+(Math.max(a,b)+0.05)/(Math.min(a,b)+0.05));
    }
    // #838: in the All grid a focused card on the first column and row stays
    // whole inside the grid's clip.
    function test_all_grid_keeps_a_focused_card_whole() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();keyClick(Qt.Key_Down);
        const albums=findChild(view,"musicShelf-albums");tryCompare(albums,"activeFocus",true);
        for(let i=0;i<3;i++)keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");const grid=findChild(all,"musicAllGrid");tryCompare(grid,"activeFocus",true);
        const card=grid.currentItem;tryCompare(card,"scale",Ui.Theme.contentFocusScale);
        const mat=findChild(card,"focusMat");tryCompare(mat,"opacity",1);
        const clip=findChild(all,"musicAllClip");const box=clip.mapToItem(view,0,0);const at=mat.mapToItem(view,0,0);
        verify(at.x-Ui.Theme.rimThickness>=box.x,"left "+at.x+" clip "+box.x);
        verify(at.y-Ui.Theme.rimThickness>=box.y,"top "+at.y+" clip "+box.y);
        verify(!findChild(all,"musicAllTopFade").visible);
    }
    // Review of #848: a focused wide station card on the first or last
    // column of the All grid stays whole inside its clip.
    function test_all_grid_keeps_a_wide_card_whole() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const stations=[];for(let i=0;i<6;i++)stations.push(room.card("account","station:"+i,"station","Station "+i));
        room.shelves=[{id:"stations",title:"Stations",kind:"station",entries:stations}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();
        const shelf=findChild(view,"musicShelf-stations");tryCompare(shelf,"activeFocus",true);
        findChild(shelf,"musicShelfCards").currentIndex=6;wait(0);keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");const grid=findChild(all,"musicAllGrid");tryCompare(grid,"activeFocus",true);
        const clip=findChild(all,"musicAllClip");const box=clip.mapToItem(view,0,0);
        const columns=Math.floor(grid.width/grid.cellWidth);verify(columns>=2,"columns "+columns);
        for(const index of [0,columns-1]) {
            grid.currentIndex=index;const card=grid.currentItem;tryCompare(card,"activeFocus",true);
            tryCompare(card,"scale",Ui.Theme.contentFocusScale);
            const mat=findChild(card,"focusMat");tryCompare(mat,"opacity",1);
            const at=mat.mapToItem(view,0,0),end=mat.mapToItem(view,mat.width,0);
            verify(at.x-Ui.Theme.rimThickness>=box.x,index+": left "+at.x+" clip "+box.x);
            verify(end.x+Ui.Theme.rimThickness<=box.x+clip.width,index+": right "+end.x+" clip "+(box.x+clip.width));
            verify(at.y-Ui.Theme.rimThickness>=box.y,index+": top "+at.y+" clip "+box.y);
        }
    }
    // #843 (shell part): an unavailable card shows one glyph and dims as one
    // layer; a playing station without its own cover shows the playing
    // track's, then the neutral chip with the station glyph.
    function test_card_glyph_and_playing_station_cover() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const gone=Object.assign(room.card("local","station:all","station","All my music"),{available:false});
        room.shelves=[{id:"stations",title:"Stations",kind:"station",entries:[room.card("account","station:mix","station","Fixture mix"),gone]}];
        catalog.activeSourceId="account";
        catalog.activeState={session:"s",playback:"playing",station:{id:"station:mix",title:"Fixture mix"},
            current:{track:{id:"t",title:"T",image:"image://fixture/track-cover"}},actions:["pause"],upcoming:[]};
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();
        const stations=findChild(view,"musicShelf-stations");tryCompare(stations,"activeFocus",true);
        const cards=[];const list=findChild(stations,"musicShelfCards");
        for(let i=0;i<2;i++){list.currentIndex=i;wait(0);cards.push(findChild(list.currentItem,"musicShelfCard"));}
        // The playing station without its own cover: the track's cover.
        compare(cards[0].playing,true);compare(findChild(cards[0],"musicCardArtwork").source.toString(),"image://fixture/track-cover");
        // A station that does not play keeps the neutral chip.
        compare(cards[1].playing,false);compare(findChild(cards[1],"musicCardArtwork").source.toString(),"");
        // One glyph on the unavailable card, and the card dims as one layer.
        // Glyphs on the artwork; the caption's source mark is not one.
        const glyphs=item=>{let n=0;const walk=node=>{for(const child of node.children){if(child.objectName==="musicCardSource")continue;if(child.visible&&String(child).indexOf("LineIcon")>=0)n++;walk(child);}};walk(item);return n;};
        compare(glyphs(cards[1]),1);compare(cards[1].opacity,0.6);compare(cards[1].layer.enabled,true);
        compare(cards[0].layer.enabled,false);
        // Codex review of #848: the dimmed layer takes in the focus mat and
        // rim, which paint outside the card.
        const bleed=Ui.Theme.matThickness+Ui.Theme.rimThickness;const rect=cards[1].layer.sourceRect;
        verify(rect.x<=-bleed&&rect.y<=-bleed&&rect.width>=cards[1].width+2*bleed&&rect.height>=cards[1].height+2*bleed,
               JSON.stringify(rect));
        // Codex review of #848: the All grid shows the playing station with
        // the playing track's cover too.
        list.currentIndex=2;wait(0);keyClick(Qt.Key_Return);compare(view.mode,"all");
        const grid=findChild(findChild(view,"musicAll"),"musicAllGrid");tryCompare(grid,"activeFocus",true);
        grid.currentIndex=0;wait(0);
        compare(grid.currentItem.playing,true);
        compare(findChild(grid.currentItem,"musicCardArtwork").source.toString(),"image://fixture/track-cover");
    }
    // #800: a late source adds its entries without moving focus; an entry
    // that leaves hands focus to its neighbour.
    function test_late_entries_keep_focus_and_a_dropped_source_moves_it() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();keyClick(Qt.Key_Down);
        const albums=findChild(view,"musicShelf-albums");tryCompare(albums,"activeFocus",true);
        keyClick(Qt.Key_Right);compare(albums.focusedEntry.key,"account\nalbum:1");
        // A late page puts a dated entry first in the room's order.
        const late=room.card("account","album:7","album","Late");
        room.shelves=[room.shelves[0],Object.assign({},room.shelves[1],{entries:[late].concat(room.shelves[1].entries)})];
        tryCompare(albums,"count",4);compare(albums.focusedEntry.key,"account\nalbum:1");
        compare(albums.shown[3].key,"account\nalbum:7");
        // The service drops out: its entries leave and focus moves on.
        room.shelves=[{id:"albums",title:"Albums",kind:"album",entries:[room.card("local","album:1","album","First"),room.card("local","album:2","album","Second")]}];
        const left=findChild(view,"musicShelf-albums");tryVerify(()=>left && left.count===2);
        tryVerify(()=>left.activeFocus);verify(left.focusedEntry!==null);
    }
    // #800: Refresh and Disconnect live only in the Sources sheet; Disconnect
    // asks first with Cancel as the initial focus.
    function test_sources_sheet_holds_the_housekeeping() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();const sources=findChild(view,"musicSources");Ui.Focus.take(sources);keyClick(Qt.Key_Return);
        const sheet=findChild(view,"musicSourcesOverlay");tryCompare(sheet,"visible",true);
        const names=findChild(sheet,"musicSourceName");verify(names);
        const scan=findChild(sheet,"musicSourceAction-scan");tryCompare(scan,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(room.commands,["local:scan"]);
        // Review of #812: the republished rows keep the focused pill.
        wait(20);compare(scan.activeFocus,true);
        // Down reaches the service's row; its Disconnect is right of Refresh.
        keyClick(Qt.Key_Down);keyClick(Qt.Key_Right);
        tryCompare(findChild(sheet,"musicSourceAction-disconnect"),"activeFocus",true);keyClick(Qt.Key_Return);
        tryCompare(findChild(sheet,"musicKeepSource"),"activeFocus",true);compare(room.commands,["local:scan"]);
        keyClick(Qt.Key_Right);keyClick(Qt.Key_Return);compare(room.commands,["local:scan","account:disconnect"]);
        keyClick(Qt.Key_Escape);tryCompare(sheet,"visible",false);tryCompare(sources,"activeFocus",true);
    }
    // #842: Turn off moves a source to the Off group, where Turn on brings
    // it back; turning off the source that plays asks first, with Cancel
    // focused, then stops its music.
    function test_sources_can_be_turned_off_and_on() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        catalog.activeSourceId="local";
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();const sources=findChild(view,"musicSources");Ui.Focus.take(sources);keyClick(Qt.Key_Return);
        const sheet=findChild(view,"musicSourcesOverlay");tryCompare(sheet,"visible",true);
        // The service's row: Refresh, Disconnect, Turn off.
        tryCompare(findChild(sheet,"musicSourceAction-scan"),"activeFocus",true);
        keyClick(Qt.Key_Down);keyClick(Qt.Key_Right);keyClick(Qt.Key_Right);
        const off=()=>{const rows=[];const walk=n=>{for(const c of n.children){if(c.objectName==="musicSourceAction-off")rows.push(c);walk(c);}};walk(sheet);return rows;};
        tryVerify(()=>off().some(pill=>pill.activeFocus));compare(off()[0].text,"Turn off");
        keyClick(Qt.Key_Return);compare(room.commands,["account:off"]);
        // It moves to the Off group, last, and focus stays on it: Turn on.
        const on=findChild(sheet,"musicSourceAction-on");tryVerify(()=>on&&on.activeFocus);compare(on.text,"Turn on");
        const headings=()=>{const out=[];const walk=n=>{for(const c of n.children){if(c.objectName==="musicSourcesOffGroup"&&c.visible)out.push(c);walk(c);}};walk(sheet);return out;};
        tryVerify(()=>headings().length===1);
        compare(room.sources[1].id,"account");compare(room.sources[1].status,"off");
        keyClick(Qt.Key_Return);compare(room.commands,["account:off","account:on"]);
        tryVerify(()=>!findChild(sheet,"musicSourceAction-on"));
        // The source that plays: Turn off asks first, Cancel focused.
        Ui.Focus.take(off()[0]);keyClick(Qt.Key_Return);
        const keep=findChild(sheet,"musicKeepPlaying");tryCompare(keep,"activeFocus",true);
        compare(findChild(sheet,"musicTurnOffDialog").visible,true);compare(room.commands.length,2);
        keyClick(Qt.Key_Return);tryCompare(findChild(sheet,"musicTurnOffDialog"),"visible",false);compare(room.commands.length,2);
        Ui.Focus.take(off()[0]);keyClick(Qt.Key_Return);tryCompare(keep,"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(findChild(sheet,"musicTurnOffSource"),"activeFocus",true);
        catalog.commands=[];keyClick(Qt.Key_Return);
        compare(catalog.commands,["stop"]);compare(room.commands[2],"local:off");
    }
    // #842: with every source off, the room points to Sources.
    function test_every_source_off_points_to_sources() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.shelves=[];room.sources=room.sources.map(row=>Object.assign({},row,{status:"off",actions:[],browsable:false}));
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();
        const connect=findChild(view,"musicConnectFirst");tryCompare(connect,"activeFocus",true);
        compare(findChild(view,"musicHeroTitle").text,"Music sources are off");
        compare(findChild(view,"musicHeroMeta").text,"Turn a source on in Sources.");
        compare(connect.text,"Sources");
        keyClick(Qt.Key_Return);tryCompare(findChild(view,"musicSourcesOverlay"),"visible",true);
        tryCompare(findChild(view,"musicSourceAction-on"),"activeFocus",true);
    }
    // #800 first use: no connected source, so the hero offers Connect, which
    // opens the Sources sheet, and no shelf shows.
    function test_first_use_offers_connect() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.connected=false;room.shelves=[];room.sources=[{id:"account",name:"Fixture service",local:false,status:"signin",reason:"",actions:["connect"],busy:false}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();
        const connect=findChild(view,"musicConnectFirst");tryCompare(connect,"activeFocus",true);
        compare(findChild(view,"musicHeroTitle").text,"Connect a source");compare(findChild(view,"musicShelves").visible,false);
        keyClick(Qt.Key_Return);tryCompare(findChild(view,"musicSourcesOverlay"),"visible",true);
        tryCompare(findChild(view,"musicSourceAction-connect"),"activeFocus",true);
    }
    // #800: All opens the full list with the source tabs starting at All;
    // a detail opened from the room browses the card's own source, and
    // Back returns to the room with focus on that card (M16).
    function test_all_list_and_detail_return_to_the_room() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();keyClick(Qt.Key_Down);
        const albums=findChild(view,"musicShelf-albums");tryCompare(albums,"activeFocus",true);
        for(let i=0;i<3;i++)keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");tryCompare(findChild(all,"musicAllGrid"),"activeFocus",true);
        compare(all.entries.length,3);compare(findChild(all,"musicAllTab-0").text,"● All");
        Ui.Focus.take(findChild(all,"musicAllTab-2"));keyClick(Qt.Key_Return);compare(all.entries.length,1);
        // Back returns to the shelf's All tile.
        keyClick(Qt.Key_Escape);compare(view.mode,"root");
        tryVerify(()=>findChild(view,"musicShelf-albums").activeFocus);compare(findChild(view,"musicShelf-albums").focusedEntry,null);
        // Open the service's copy of album:1 from the room.
        keyClick(Qt.Key_Left);keyClick(Qt.Key_Left);
        compare(findChild(view,"musicShelf-albums").focusedEntry.key,"account\nalbum:1");
        keyClick(Qt.Key_Return);compare(view.mode,"catalogue");compare(catalog.sourceId,"account");
        compare(view.parentId,"album:1");compare(catalog.browsed[catalog.browsed.length-1],"album:1");
        keyClick(Qt.Key_Escape);compare(view.mode,"root");
        tryVerify(()=>findChild(view,"musicShelf-albums").activeFocus);
        compare(findChild(view,"musicShelf-albums").focusedEntry.key,"account\nalbum:1");
    }
    // #800: a station or track in the room plays from its own source; over
    // a live session that asks to replace it first.
    function test_room_station_plays_from_its_source() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        catalog.state={account:"connected",facets:["selections"],actions:["play","radio"]};
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();findChild(view,"musicRoom").focusInitial();
        tryCompare(findChild(view,"musicShelf-stations"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.sourceId,"account");tryCompare(view,"pendingPlay","station:mix");
        // Review of #812: the confirm dialog sits above the room, Cancel
        // returns focus to the station, and a confirmed station starts as radio.
        const dialog=findChild(view,"musicReplaceDialog");verify(dialog.z>findChild(view,"musicRoom").z);
        keyClick(Qt.Key_Escape);compare(view.pendingPlay,"");
        const stations=findChild(view,"musicShelf-stations");tryVerify(()=>stations.activeFocus);
        compare(stations.focusedEntry.key,"account\nstation:mix");
        catalog.commands=[];keyClick(Qt.Key_Return);tryCompare(view,"pendingPlay","station:mix");
        findChild(view,"musicReplaceQueue").activated();compare(catalog.commands,["radio"]);compare(catalog.played,["station:mix"]);
    }
    // Review of #812: the shell focuses the section through the view; the
    // room then holds focus on a card and the D-pad moves it.
    function test_view_focus_reaches_the_room() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        tryVerify(()=>findChild(view,"musicShelf-stations").activeFocus);
        keyClick(Qt.Key_Down);tryVerify(()=>findChild(view,"musicShelf-albums").activeFocus);
        // Review of #812: Back from a player opened from a detail returns to
        // the room, and the next Back leaves Music.
        let backs=0;view.backRequested.connect(()=>backs++);
        view.parentId="album:1";view.parentTitle="First";view.mode="player";
        keyClick(Qt.Key_Escape);compare(view.mode,"root");compare(view.parentId,"");
        // Not opened from a card: focus starts on the first shelf.
        tryVerify(()=>findChild(view,"musicShelf-stations").activeFocus);
        keyClick(Qt.Key_Escape);compare(backs,1);
    }
    // Review of #812: with focus on a shelf's All tile, a late card does not
    // take it.
    function test_all_tile_keeps_focus_when_cards_join() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Right);
        const stations=findChild(view,"musicShelf-stations");compare(stations.focusedEntry,null);
        room.shelves=[Object.assign({},room.shelves[0],{entries:room.shelves[0].entries.concat([room.card("local","station:late","station","Late")])}),room.shelves[1]];
        tryCompare(stations,"count",2);compare(stations.focusedEntry,null);
    }
    // Review of #812: the shell opens Music by giving the view focus through
    // its scope, without calling focusInitial(); a card still takes it.
    function test_focus_routed_by_scope_reaches_a_card() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room,visible:false});verify(view);
        view.visible=true;view.forceActiveFocus();
        tryVerify(()=>findChild(view,"musicShelf-stations").activeFocus);
        keyClick(Qt.Key_Down);tryVerify(()=>findChild(view,"musicShelf-albums").activeFocus);
        // A source that leaves while its pill has focus hands focus on.
        Ui.Focus.take(findChild(view,"musicSources"));keyClick(Qt.Key_Return);
        const sheet=findChild(view,"musicSourcesOverlay");tryCompare(sheet,"visible",true);
        // Down moves to the service's row, which then leaves.
        keyClick(Qt.Key_Down);tryVerify(()=>!findChild(sheet,"musicSourceAction-scan").activeFocus);
        room.sources=[room.sources[0]];
        tryCompare(findChild(sheet,"musicSourceAction-scan"),"activeFocus",true);
    }
    // Codex review of #812: a page without a card of this shelf's kind
    // (selections mix kinds) does not stop paging; tabs show only sources
    // that can browse; a connected source without music is not first use.
    function test_all_list_pages_past_empty_pages_and_tabs_browsable_sources() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.sources=room.sources.concat([{id:"expired",name:"Fixture expired",local:false,status:"signin",reason:"",actions:["connect"],busy:false,browsable:false}]);
        room.pending=[[],[room.card("account","album:late","album","Late")]];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Down);
        const albums=findChild(view,"musicShelf-albums");tryVerify(()=>albums.activeFocus);
        for(let i=0;i<3;i++)keyClick(Qt.Key_Right);keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");
        tryCompare(room,"loads",2);tryCompare(all.entries,"length",4);
        compare(findChild(all,"musicAllTab-3"),null);compare(findChild(all,"musicAllTab-2").text,"Fixture service");
    }
    function test_connected_source_without_music_is_not_first_use() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.connected=false;room.shelves=[];room.sources=[{id:"account",name:"Fixture service",local:false,status:"connected",reason:"",actions:["disconnect"],busy:false,browsable:false}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        compare(findChild(view,"musicHeroTitle").text,"No music yet");compare(findChild(view,"musicConnectFirst").text,"Sources");
    }
    // Review of #812: an empty room is never shown bare. A local library
    // without music, or a lone expired service, still lists facets, so the
    // state comes from the shelves and the source rows.
    function test_empty_room_explains_itself() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.shelves=[];room.sources=[{id:"local",name:"Fixture music",local:true,status:"connected",reason:"",actions:["scan"],busy:false,browsable:true}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        compare(findChild(view,"musicHeroTitle").text,"No music yet");compare(findChild(view,"musicHeroMeta").text,"Add folders with music in Settings.");
        const action=findChild(view,"musicConnectFirst");tryCompare(action,"activeFocus",true);compare(action.text,"Sources");
        room.sources=[{id:"account",name:"Fixture service",local:false,status:"signin",reason:"",actions:["authorize"],busy:false,browsable:true}];
        compare(findChild(view,"musicHeroTitle").text,"Connect a source");compare(action.text,"Connect a source");
    }
    // Codex review of #812: the Sources rows scroll with focus, so a lower
    // source's pills are on screen when they hold focus.
    function test_sources_sheet_scrolls_with_focus() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const rows=[];for(let i=0;i<8;i++)rows.push({id:"s"+i,name:"Fixture "+i,local:false,status:"connected",reason:"",actions:["scan"],busy:false,browsable:true});
        room.sources=rows;
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();Ui.Focus.take(findChild(view,"musicSources"));keyClick(Qt.Key_Return);
        const sheet=findChild(view,"musicSourcesOverlay");tryCompare(sheet,"visible",true);
        for(let i=0;i<7;i++)keyClick(Qt.Key_Down);
        const area=findChild(sheet,"musicSourceRows");tryVerify(()=>area.contentY>0);
        // Delta review of #812: a pill keeps room for its focus scale.
        tryVerify(()=>{const pill=view.Window.window.activeFocusItem;const at=pill.mapToItem(area,0,0);return at.x>=8;});
        const focused=view.Window.window.activeFocusItem;verify(focused.objectName==="musicSourceAction-scan");
        tryVerify(()=>{const at=focused.mapToItem(area,0,0);return at.y>=0 && at.y+focused.height<=area.height;});
    }
    // Review of #812: when an empty room fills, focus leaves the hidden
    // first-use action for a card, and the D-pad works.
    function test_room_that_fills_moves_focus_off_the_empty_state() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const shelves=room.shelves;room.shelves=[];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        const action=findChild(view,"musicConnectFirst");tryCompare(action,"activeFocus",true);
        keyClick(Qt.Key_Up);tryCompare(findChild(view,"musicSources"),"activeFocus",true);
        Ui.Focus.take(action);tryCompare(action,"activeFocus",true);
        room.shelves=shelves;
        tryVerify(()=>findChild(view,"musicShelf-stations") && findChild(view,"musicShelf-stations").activeFocus);
        keyClick(Qt.Key_Down);tryVerify(()=>findChild(view,"musicShelf-albums").activeFocus);
    }
    // Codex review of #812: the All list's tabs scroll with focus, so the
    // ordering controls past many sources stay reachable on screen.
    function test_all_tabs_scroll_with_focus() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const rows=[];for(let i=0;i<10;i++)rows.push({id:"s"+i,name:"Fixture service with a long name "+i,local:false,status:"connected",reason:"",actions:[],busy:false,browsable:true});
        room.sources=rows;
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Down);
        for(let i=0;i<3;i++)keyClick(Qt.Key_Right);keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");Ui.Focus.take(findChild(all,"musicAllTab-0"));
        for(let i=0;i<12;i++)keyClick(Qt.Key_Right);
        const az=findChild(all,"musicAllAlphabetical");tryCompare(az,"activeFocus",true);
        const area=findChild(all,"musicAllTabArea");
        tryVerify(()=>{const at=az.mapToItem(area,0,0);return at.x>=0 && at.x+az.width<=area.width;});
        // Delta review of #812: the focused pill, scaled up, is not clipped.
        Ui.Focus.take(findChild(all,"musicAllTab-0"));const first=findChild(all,"musicAllTab-0");
        tryVerify(()=>{const at=first.mapToItem(area,0,0);return at.x>=8 && at.y>=8 && at.y+first.height<=area.height-8;});
    }
    // Codex review of #812: before any source is known, the room is not the
    // first use.
    function test_no_sources_yet_is_not_first_use() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.connected=false;room.shelves=[];room.sources=[];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        compare(findChild(view,"musicRoom").firstUse,false);compare(findChild(view,"musicConnectFirst").visible,false);
    }
    // Codex review of #812: Up and Down in the Sources sheet move between
    // rows' first pills. Since #842 every row has one: a source without
    // housekeeping can still be turned off.
    function test_sources_sheet_up_skips_actionless_rows() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        room.sources=[{id:"a",name:"Fixture A",local:false,status:"connected",reason:"",actions:["scan"],busy:false,browsable:true},
                      {id:"b",name:"Fixture B",local:false,status:"connected",reason:"",actions:[],busy:false,browsable:true},
                      {id:"c",name:"Fixture C",local:false,status:"connected",reason:"",actions:["disconnect"],busy:false,browsable:true}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();Ui.Focus.take(findChild(view,"musicSources"));keyClick(Qt.Key_Return);
        const sheet=findChild(view,"musicSourcesOverlay");tryCompare(sheet,"visible",true);
        const scan=findChild(sheet,"musicSourceAction-scan");tryCompare(scan,"activeFocus",true);
        const offs=()=>{const out=[];const walk=n=>{for(const c of n.children){if(c.objectName==="musicSourceAction-off")out.push(c);walk(c);}};walk(sheet);return out;};
        keyClick(Qt.Key_Down);tryVerify(()=>offs()[1].activeFocus);
        keyClick(Qt.Key_Down);tryCompare(findChild(sheet,"musicSourceAction-disconnect"),"activeFocus",true);
        keyClick(Qt.Key_Up);tryVerify(()=>offs()[1].activeFocus);
        keyClick(Qt.Key_Up);tryCompare(scan,"activeFocus",true);
    }
    // Codex review of #812: shelves that arrive above the focused one keep
    // the focused shelf in view.
    function test_focused_shelf_stays_in_view_when_shelves_arrive_above() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const albums=room.shelves[1];room.shelves=[albums];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        const shelf=findChild(view,"musicShelf-albums");tryVerify(()=>shelf.activeFocus);
        room.shelves=[{id:"stations",title:"Stations",kind:"station",entries:[room.card("account","station:mix","station","Fixture mix")]},
                      {id:"forYou",title:"For you",kind:"playlist",entries:[room.card("account","playlist:1","playlist","Mix")]},albums];
        const area=findChild(view,"musicShelves");
        // Once the column has laid the new shelves out above it.
        tryVerify(()=>shelf.y>0);
        tryVerify(()=>shelf.activeFocus && shelf.y>=area.contentY && shelf.y+shelf.height<=area.contentY+area.height);
    }
    // Codex review of #812: a source that answers with no actions ends a
    // pending room play; a later state with play does not start it.
    function test_room_play_ends_when_the_source_offers_nothing() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        catalog.state={};
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();tryVerify(()=>findChild(view,"musicShelf-stations").activeFocus);
        keyClick(Qt.Key_Return);compare(catalog.sourceId,"account");
        catalog.state={revision:1,account:"connected",facets:["selections"],actions:[]};
        wait(20);verify(view.roomPlay===null);
        catalog.state={revision:2,account:"connected",facets:["selections"],actions:["play","radio"]};
        wait(20);compare(view.pendingPlay,"");compare(catalog.commands,[]);
    }
    // Codex review of #812: a source row that changes keeps the focused tab;
    // a tab that leaves hands focus to All.
    function test_all_tabs_keep_focus_when_sources_change() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Down);
        for(let i=0;i<3;i++)keyClick(Qt.Key_Right);keyClick(Qt.Key_Return);compare(view.mode,"all");
        const all=findChild(view,"musicAll");const service=findChild(all,"musicAllTab-2");
        Ui.Focus.take(service);tryCompare(service,"activeFocus",true);
        room.sources=room.sources.map(row=>Object.assign({},row,{busy:true}));
        wait(20);compare(service.activeFocus,true);
        room.sources=[room.sources[0]];
        tryCompare(findChild(all,"musicAllTab-0"),"activeFocus",true);
    }
    // #801 detail (brief mh-02): opened from the room, it names the source in
    // its eyebrow, shows the pills by capability, never plays on opening,
    // numbers its rows and marks the row that plays.
    function test_detail_from_the_room() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        catalog.state={account:"connected",facets:["albums"],actions:["play","radio"]};
        catalog.entries=[{id:"track-0",kind:"track",title:"One",artist:"Fixture artist",duration_us:185000000,available:true},
                         {id:"track-2",kind:"track",title:"Two",artist:"Guest",duration_us:60000000,available:true}];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Down);keyClick(Qt.Key_Right);
        const card=findChild(view,"musicShelf-albums").focusedEntry;compare(card.key,"account\nalbum:1");
        keyClick(Qt.Key_Return);compare(view.mode,"catalogue");verify(view.detailView);
        compare(catalog.commands,[]);
        const header=findChild(view,"musicDetailHeader");verify(header.visible);
        compare(findChild(header,"musicDetailEyebrow").text,"Album · Fixture service");
        compare(findChild(header,"musicDetailTitle").text,"Same id");
        verify(findChild(header,"musicDetailMeta").text.indexOf("2 tracks")>=0);
        verify(findChild(header,"musicDetailListen").visible);verify(findChild(header,"musicDetailRadio").visible);
        compare(findChild(view,"musicSourcePicker").visible,false);compare(findChild(view,"musicSearch").visible,false);
        // Codex review of #819: the playlist actions follow the pills, never on them.
        catalog.state={account:"connected",facets:["albums"],actions:["play","radio","save_playlist"]};
        const save=findChild(view,"musicSavePlaylist");tryVerify(()=>save.visible);
        const radio=findChild(header,"musicDetailRadio");
        verify(save.mapToItem(view,0,0).x>=radio.mapToItem(view,0,0).x+radio.width);
        compare(save.mapToItem(view,0,0).y,radio.mapToItem(view,0,0).y);
        // Review of #819: the playlist actions join the pills for the D-pad,
        // and Down from them reaches the tracks.
        Ui.Focus.take(radio);keyClick(Qt.Key_Right);tryCompare(save,"activeFocus",true);
        keyClick(Qt.Key_Left);tryCompare(radio,"activeFocus",true);
        Ui.Focus.take(save);keyClick(Qt.Key_Down);tryCompare(findChild(view,"musicEntries"),"activeFocus",true);
        // Codex review of #819: actions that would pass the safe area (a long
        // translation) take their own line under the pills, inside it, and
        // the D-pad follows: Down from the pills, Up back, the tracks below.
        save.text="A playlist action whose translation runs far longer than the line beside the pills";
        const actions=findChild(view,"musicPlaylistActions");
        tryVerify(()=>save.mapToItem(view,0,0).y>=radio.mapToItem(view,0,radio.height).y);
        verify(save.mapToItem(view,save.width,0).x<=1920-96);
        const listing=findChild(view,"musicEntries");
        verify(listing.y>=save.mapToItem(view,0,save.height).y);
        Ui.Focus.take(radio);keyClick(Qt.Key_Down);tryCompare(save,"activeFocus",true);
        keyClick(Qt.Key_Up);tryCompare(findChild(header,"musicDetailListen"),"activeFocus",true);
        Ui.Focus.take(listing);listing.currentIndex=0;keyClick(Qt.Key_Up);tryCompare(save,"activeFocus",true);
        save.text="Save playlist";
        tryCompare(actions,"y",header.pillsY);verify(actions.beside);
        // Codex review of #819: with the actions alone, the tracks start
        // below them.
        catalog.state={account:"connected",facets:["albums"],actions:["save_playlist"]};
        tryVerify(()=>!findChild(header,"musicDetailListen").visible);wait(100);
        verify(listing.y>=actions.y+actions.height);verify(actions.x===600);
        catalog.state={account:"connected",facets:["albums"],actions:["play","radio","save_playlist"]};
        catalog.state={account:"connected",facets:["albums"],actions:["play","radio"]};
        // Review of #819: nothing overlaps the tracks, also under a title on
        // three lines, and the status line stays below them.
        const rows=findChild(view,"musicEntries"),notice=findChild(view,"musicNotice");
        compare(findChild(view,"musicListPosition").visible,false);
        verify(rows.y+rows.height<=notice.y);
        const shortEntry=view.detailEntry;
        view.detailEntry=Object.assign({},shortEntry,{title:"An extended synthetic title from the fixture album that runs over three whole lines of the detail"});
        const meta=findChild(header,"musicDetailMeta");
        // The title lays out again first (delta review of #819).
        tryCompare(findChild(header,"musicDetailTitle"),"lineCount",3);wait(100);
        verify(radio.mapToItem(view,0,0).y>=meta.mapToItem(view,0,meta.height).y);
        verify(rows.y>=radio.mapToItem(view,0,radio.height).y);
        verify(rows.y+rows.height<=notice.y);
        view.detailEntry=shortEntry;
        // One track is one track, not "1 tracks".
        const twoTracks=catalog.entries;catalog.entries=[twoTracks[0]];
        verify(meta.text.indexOf("1 track")>=0);verify(meta.text.indexOf("1 tracks")<0);
        catalog.entries=twoTracks;
        // The row of the session that plays carries the playing mark.
        catalog.activeSourceId="account";
        const list=findChild(view,"musicEntries");tryCompare(list,"count",2);
        const first=list.itemAtIndex(0);tryVerify(()=>findChild(first,"musicRowPlaying").visible);
        compare(findChild(list.itemAtIndex(1),"musicRowNumber").text,"2");
        compare(findChild(first,"musicRowDuration").text,"3:05");
        // Up from the first row reaches Listen, which starts the album.
        Ui.Focus.take(list);list.currentIndex=0;keyClick(Qt.Key_Up);
        tryCompare(findChild(header,"musicDetailListen"),"activeFocus",true);
        keyClick(Qt.Key_Return);tryCompare(view,"pendingPlay","album:1");
        keyClick(Qt.Key_Escape);keyClick(Qt.Key_Escape);compare(view.mode,"root");
        catalog.state={account:"connected",facets:["albums"],actions:["play"]};
    }
    // Codex review of #819: an empty detail focuses its first pill, as the
    // catalogue's own actions are hidden there.
    function test_empty_detail_focuses_its_pill() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        catalog.state={account:"connected",facets:["albums"],actions:["play","radio"]};
        catalog.entries=[];
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();keyClick(Qt.Key_Down);keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Return);verify(view.detailView);
        tryCompare(findChild(view,"musicDetailListen"),"activeFocus",true);
    }
    // #801 Find (MH-10): one field over every source, results as shelves with
    // source marks, a quiet line for a source still searching; a result opens
    // its detail, and Back returns to the results, then to the room.
    // #829: Find could not be left from its field with the remote's Back;
    // Up went to a hidden source picker.
    function test_find_is_left_with_back_from_its_field() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        const find=findChild(view,"musicFind"),field=findChild(view,"musicSearch");
        const enter=()=>{Ui.Focus.take(find);keyClick(Qt.Key_Return);compare(view.mode,"search");tryCompare(field,"activeFocus",true);};
        // The empty field with no results: Back returns to the room.
        enter();compare(room.searchShelves.length,0);
        keyClick(Qt.Key_Back);compare(view.mode,"root");tryCompare(find,"activeFocus",true);
        // Esc from the field leaves Find too.
        enter();keyClick(Qt.Key_Escape);compare(view.mode,"root");
        // Backspace in the field with text deletes and stays.
        enter();field.text="fix";field.cursorPosition=3;keyClick(Qt.Key_Backspace);
        compare(field.text,"fi");compare(view.mode,"search");verify(field.activeFocus);
        // Up never moves focus to a hidden item.
        keyClick(Qt.Key_Up);verify(field.activeFocus);compare(view.mode,"search");
        // With the keyboard panel open, Back closes it first; the next Back
        // leaves Find.
        keyClick(Qt.Key_Return);tryCompare(view,"typing",true);
        keyClick(Qt.Key_Back);tryCompare(view,"typing",false);compare(view.mode,"search");
        tryCompare(field,"activeFocus",true);
        keyClick(Qt.Key_Back);compare(view.mode,"root");compare(room.searchQuery,"");
    }
    // #829, review of #830: the catalogue filter field (no room) is left with
    // Back too, after its keyboard panel closes.
    function test_catalogue_filter_is_left_with_back() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog});verify(view);wait(0);
        let backs=0;view.backRequested.connect(()=>backs++);
        const field=findChild(view,"musicSearch");Ui.Focus.take(field);tryCompare(field,"activeFocus",true);
        field.text="ab";field.cursorPosition=2;keyClick(Qt.Key_Backspace);compare(field.text,"a");compare(backs,0);
        keyClick(Qt.Key_Return);tryCompare(view,"typing",true);
        keyClick(Qt.Key_Back);tryCompare(view,"typing",false);compare(backs,0);tryCompare(field,"activeFocus",true);
        keyClick(Qt.Key_Back);compare(backs,1);
    }
    function test_find_searches_every_source() {
        const catalog=createTemporaryObject(catalogFactory,suite);const room=createTemporaryObject(roomFactory,suite);
        const view=createTemporaryObject(roomViewFactory,suite,{catalog:catalog,room:room});verify(view);
        view.forceActiveFocus();view.focusInitial();
        Ui.Focus.take(findChild(view,"musicFind"));keyClick(Qt.Key_Return);compare(view.mode,"search");
        const field=findChild(view,"musicSearch");tryCompare(field,"activeFocus",true);verify(field.visible);
        compare(findChild(view,"musicSearchWhere").text,"Searching everywhere: this computer and Fixture service");
        field.text="fix";tryVerify(()=>room.searches.indexOf("fix")>=0);
        room.searchPending=["Fixture service"];
        compare(findChild(view,"musicSearchPending").text,"Fixture service is still searching…");
        room.searchShelves=[{id:"albums",title:"Albums",kind:"album",entries:[room.card("local","album:2","album","Second"),room.card("account","album:1","album","Same id")]}];
        room.searchPending=[];
        // Codex review of #819: Down from the keyboard button also reaches
        // the results.
        const results=findChild(view,"musicSearchShelf-albums");
        Ui.Focus.take(findChild(view,"musicKeyboard"));keyClick(Qt.Key_Down);tryVerify(()=>results.activeFocus);
        Ui.Focus.take(field);keyClick(Qt.Key_Down);tryVerify(()=>results.activeFocus);
        keyClick(Qt.Key_Right);compare(results.focusedEntry.key,"account\nalbum:1");
        keyClick(Qt.Key_Return);verify(view.detailView);compare(catalog.sourceId,"account");
        keyClick(Qt.Key_Escape);compare(view.mode,"search");
        tryVerify(()=>results.activeFocus);compare(results.focusedEntry.key,"account\nalbum:1");
        // Review of #819: a query typed just before a result opens is still
        // searched, and never filters the detail.
        const browses=catalog.browsed.filter(parent=>parent==="album:1").length;
        field.text="fixe";keyClick(Qt.Key_Return);verify(view.detailView);
        wait(500);compare(catalog.browsed.filter(parent=>parent==="album:1").length,browses+1);
        verify(room.searches.indexOf("fixe")>=0);
        keyClick(Qt.Key_Escape);compare(view.mode,"search");tryVerify(()=>results.activeFocus);
        // Review of #819: cancelling Replace from a track result keeps focus
        // on that result, and Back still returns to the results first.
        const found=room.searchShelves[0];
        room.searchShelves=[found,{id:"tracks",title:"Tracks",kind:"track",entries:[room.card("local","track:9","track","Nine")]}];
        keyClick(Qt.Key_Down);const trackShelf=findChild(view,"musicSearchShelf-tracks");tryVerify(()=>trackShelf.activeFocus);
        // The dialog takes focus first (delta review of #819); both Back and
        // its Cancel return to the result.
        const keepQueue=findChild(view,"musicKeepQueue");
        keyClick(Qt.Key_Return);tryCompare(view,"pendingPlay","track:9");tryCompare(keepQueue,"activeFocus",true);
        keyClick(Qt.Key_Escape);compare(view.pendingPlay,"");
        tryVerify(()=>trackShelf.activeFocus);compare(trackShelf.focusedEntry.key,"local\ntrack:9");
        keyClick(Qt.Key_Return);tryCompare(view,"pendingPlay","track:9");tryCompare(keepQueue,"activeFocus",true);
        keyClick(Qt.Key_Return);compare(view.pendingPlay,"");
        tryVerify(()=>trackShelf.activeFocus);compare(trackShelf.focusedEntry.key,"local\ntrack:9");
        compare(view.mode,"search");
        // Codex review of #819: a shelf without an All tile that loses its
        // focused last card keeps focus on its new last card.
        const two={id:"tracks",title:"Tracks",kind:"track",entries:[room.card("local","track:9","track","Nine"),room.card("local","track:10","track","Ten")]};
        room.searchShelves=[found,two];tryVerify(()=>trackShelf.count===2);
        Ui.Focus.take(trackShelf);keyClick(Qt.Key_Right);compare(trackShelf.focusedEntry.key,"local\ntrack:10");
        room.searchShelves=[found,{id:"tracks",title:"Tracks",kind:"track",entries:[two.entries[0]]}];
        tryVerify(()=>trackShelf.count===1);verify(trackShelf.focusedEntry);compare(trackShelf.focusedEntry.key,"local\ntrack:9");
        Ui.Focus.take(results);room.searchShelves=[found];
        // Codex review of #819: a focused shelf that leaves hands focus to
        // the shelf in its place, and the last one to the field.
        const albums=room.searchShelves[0];
        room.searchShelves=[albums,{id:"tracks",title:"Tracks",kind:"track",entries:[room.card("local","track:1","track","One")]}];
        keyClick(Qt.Key_Down);const tracks=findChild(view,"musicSearchShelf-tracks");tryVerify(()=>tracks.activeFocus);
        room.searchShelves=[albums];tryVerify(()=>findChild(view,"musicSearchShelf-albums").activeFocus);
        room.searchShelves=[];tryCompare(field,"activeFocus",true);
        // Back from Find returns to the Find button (review of #819).
        keyClick(Qt.Key_Escape);compare(view.mode,"root");compare(room.searchQuery,"");
        tryCompare(findChild(view,"musicFind"),"activeFocus",true);
    }

    // #804: a pinned station on Favourites starts its station; one whose
    // source is away opens the tile actions instead of starting anything.
    function test_a_pinned_station_starts_from_favourites() {
        const pin={id:"pin-1",title:"Fixture station",musicStation:true,providerId:"fixture.music",
                   stationId:"station:library",launchable:true,pinned:true,monogram:"F"};
        const away=Object.assign({},pin,{id:"pin-2",launchable:false,unavailable:true,subtitle:"Music source not connected"});
        const home=createTemporaryObject(homeFactory,suite,{favoriteEntries:[pin,away],actionsEnabled:true});verify(home);
        const started=[];home.musicStationRequested.connect((provider,station,title)=>started.push([provider,station,title]));
        home.activate("pin-1");
        compare(started,[["fixture.music","station:library","Fixture station"]]);
        home.activate("pin-2");
        compare(started.length,1);compare(home.tileActionsOpen,true);
    }
}
