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
            property int loads: 0
            // As the catalogue: a page in flight makes it busy.
            property bool busyOnLoad: false
            function loadMore() {if(busy)return;loads++;if(busyOnLoad)busy=true;}
            function command(action,id,occurrence,position) {commands=commands.concat([action]);played=played.concat([id||""]);}
            property var positions: []
            property var occurrences: []
            function transport(action,id,occurrence,position) {commands=commands.concat([action]);positions=positions.concat([position]);occurrences=occurrences.concat([occurrence||""]);}
        }
    }
    Component {id:viewFactory;Music.MusicView {width:1920;height:1080}}
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
        view.openPlayer();tryCompare(findChild(view,"musicElapsed"),"text","1:05 / 3:05");
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
    function test_player_heading_matches_loading_paused_and_failed_state() {
        const catalog=createTemporaryObject(catalogFactory,suite);
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,mode:"player"});verify(view);
        const heading=findChild(view,"musicHeading");verify(heading);
        compare(heading.text,"Now playing");
        for(const state of [{playback:"playing",error:"Unavailable",title:"Now playing"},{playback:"paused",error:"Unavailable",title:"Paused"},{playback:"loading",title:"Loading music…"},{playback:"paused",title:"Paused"},{playback:"stopped",title:"Stopped"},{playback:"stopped",error:"Unavailable",title:"Stopped"}]) {
            catalog.activeState={playback:state.playback,error:state.error,actions:[],upcoming:[]};
            tryCompare(heading,"text",state.title);
        }
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
        const stop=findChild(view,"musicStop");verify(stop);compare(stop.iconName,"stop");compare(stop.mapToItem(controls,0,0).y,button.parent.mapToItem(controls,0,0).y);compare(button.iconName,"pause");verify(button.x+button.width<=button.parent.width);
        verify(controls.x+controls.width<=controls.parent.width);
        Ui.Focus.take(button);keyClick(Qt.Key_Right);
        tryCompare(findChild(view,"musicNext"),"activeFocus",true);
        keyClick(Qt.Key_Right);tryCompare(findChild(view,"musicQueue"),"activeFocus",true);
        keyClick(Qt.Key_Left);keyClick(Qt.Key_Left);tryCompare(button,"activeFocus",true);
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
        compare(home.navIcon(2),"volume_up");compare(home.navIcon(3),"download");
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
        tryCompare(findChild(view,"musicStop"),"activeFocus",true);
        keyClick(Qt.Key_Return);compare(catalog.commands[0],"stop");
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
        compare(findChild(view,"musicHeading").text,"Stopped");
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
        compare(iconOf(findChild(view,"musicQueue")).name,"queue_music");
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
        // A track loading disables it under focus: Stop holds focus and
        // hands it back once Play/Pause works again.
        const stop=findChild(view,"musicStop");
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
        for(const name of ["musicLike","musicDislike","musicPrevious","musicPlayPause","musicNext","musicQueue","musicSeekButton","musicStop","musicRadio"]) {
            const button=findChild(view,name);verify(button,name);Ui.Focus.take(button);tryCompare(button,"activeFocus",true);
            keyClick(Qt.Key_Down);compare(button.activeFocus,true,name+": Down stays in place");
            keyClick(Qt.Key_Up);tryCompare(seek,"activeFocus",true);
        }
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
        const view=createTemporaryObject(viewFactory,suite,{catalog:catalog,parentId:"album:550",parentKind:"album",parentTitle:"Deep"});
        verify(view);wait(0);
        const page=from=>{const rows=[];for(let i=from;i<from+100;i++)rows.push({id:"album:"+i,kind:"album",title:"A"+i,available:true});return rows;};
        view.back();catalog.more=true;
        let shown=page(0);catalog.entries=shown;
        for(let p=1;p<=5;p++) {
            tryCompare(catalog,"loads",p);compare(catalog.busy,true);
            // Status polls while the page loads.
            for(let i=0;i<3;i++)view.restoreOpener();
            compare(catalog.loads,p);compare(view.restorePages,p);
            shown=shown.concat(page(p*100));catalog.more=p<5;catalog.entries=shown;catalog.busy=false;
        }
        const list=findChild(view,"musicEntries");
        tryCompare(list,"currentIndex",550);tryCompare(list,"activeFocus",true);
        compare(catalog.loads,5);compare(view.restoreEntity,"");
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
        catalog.errorText="The music source did not respond.";compare(view.restoreEntity,"");
        catalog.errorText="";
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
}
