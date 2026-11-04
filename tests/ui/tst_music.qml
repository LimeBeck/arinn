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
            property bool busy: false
            property bool transportBusy: busy
            property bool more: false
            property var commands: []
            function choose(id) { sourceId=id; }
            property var browsed: []
            property var played: []
            function browse(facet,parent,search) {browsed=browsed.concat([parent]);}
            function loadMore() {}
            function command(action,id,occurrence,position) {commands=commands.concat([action]);played=played.concat([id||""]);}
            property var positions: []
            property var occurrences: []
            function transport(action,id,occurrence,position) {commands=commands.concat([action]);positions=positions.concat([position]);occurrences=occurrences.concat([occurrence||""]);}
        }
    }
    Component {id:viewFactory;Music.MusicView {width:1920;height:1080}}
    Component {id:homeFactory;Ui.HomeView {}}
    Component {id:rowFactory;Ui.HomeRow {width:1600;entries:[{id:"music",title:"Fixture",secondaryIcon:"pause",secondaryText:"Pause"}]}}
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
        // services stays visible and Return stays hidden without an activity.
        compare(findChild(home,"nav-6").text,"Open services");compare(findChild(home,"nav-6").visible,true);
        compare(findChild(home,"nav-7").text,"Return to activity");compare(findChild(home,"nav-7").visible,false);
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

}
