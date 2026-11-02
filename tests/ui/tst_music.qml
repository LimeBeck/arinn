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
            property bool more: false
            property var commands: []
            function choose(id) { sourceId=id; }
            function browse(facet,parent,search) {}
            function loadMore() {}
            function command(action,id,occurrence,position) {commands=commands.concat([action]);}
            function transport(action,id,occurrence,position) {commands=commands.concat([action]);}
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
        home.musicTitle="";compare(home.routes.length,5);compare(home.routes[2],"downloads");
        compare(findChild(home,"nav-5").visible,true);compare(findChild(home,"nav-6").visible,true);
    }
}
