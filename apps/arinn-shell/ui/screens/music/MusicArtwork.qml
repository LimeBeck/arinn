// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui
import "../../artwork.js" as Artwork
Item {
    id: root
    property url source: ""
    property real radius: Ui.Theme.stillRadius
    readonly property bool loaded: picture.status === Image.Ready
    property int retryInterval: 1000
    property int attempts: 0
    property bool retryBlank: false
    function retryAfterFailure() {
        if (visible && source.toString().length && attempts < 6 && !retry.running) retry.start();
    }
    onSourceChanged: { retry.stop(); attempts=0; retryBlank=false; }
    onVisibleChanged: { if (!visible) retry.stop(); else if (picture.status===Image.Error) retryAfterFailure(); }
    Timer {
        id: retry; objectName:"musicArtworkRetry"; interval: Math.min(30000,root.retryInterval*Math.pow(2,root.attempts))
        onTriggered: {
            if (!root.visible || root.attempts>=6) return;
            root.attempts++;
            root.retryBlank=true;
            Qt.callLater(()=>root.retryBlank=false);
        }
    }
    Rectangle {anchors.fill:parent;radius:root.radius;color:Ui.Theme.plateObject}
    Image {
        id:picture;anchors.fill:parent;source:root.retryBlank ? "" : root.source;asynchronous:true;cache:false;visible:false
        fillMode:Image.PreserveAspectCrop;sourceSize:Artwork.sourceSize(source,width,height)
        onStatusChanged:{frame.syncSource();if(status===Image.Error)root.retryAfterFailure();}
        onSourceChanged:Qt.callLater(frame.syncSource)
    }
    Ui.LineIcon {anchors.centerIn:parent;width:Math.min(root.width,root.height)*0.36;height:width;name:"music_note";color:Ui.Theme.textHint;visible:picture.status!==Image.Ready}
    Canvas {
        id:frame;anchors.fill:parent
        property string loadedSource:""
        function syncSource() {
            const next=picture.status===Image.Ready ? picture.source.toString() : "";
            if(loadedSource!==next){if(loadedSource.length)unloadImage(loadedSource);loadedSource=next;}
            if(available && next.length && !isImageLoaded(next) && !isImageLoading(next) && !isImageError(next))Artwork.loadCopy(frame,picture);
            requestPaint();
        }
        onAvailableChanged:syncSource()
        onImageLoaded:requestPaint()
        onPaint:{const ctx=getContext("2d");ctx.reset();if(loadedSource.length && picture.status===Image.Ready && isImageLoaded(loadedSource)){Artwork.clipRounded(ctx,width,height,root.radius);Artwork.drawCover(ctx,loadedSource,picture.implicitWidth,picture.implicitHeight,width,height);}}
    }
}
