// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui
import "../../artwork.js" as Artwork
Item {
    id: root
    property url source: ""
    property real radius: 24
    Rectangle {anchors.fill:parent;radius:root.radius;color:Ui.Theme.plateObject}
    Image {
        id:picture;anchors.fill:parent;source:root.source;asynchronous:true;visible:false
        fillMode:Image.PreserveAspectCrop;sourceSize:Artwork.sourceSize(source,width,height)
        onStatusChanged:frame.syncSource()
        onSourceChanged:Qt.callLater(frame.syncSource)
    }
    MusicIcon {anchors.centerIn:parent;width:Math.min(root.width,root.height)*0.36;height:width;name:"music";color:Ui.Theme.textHint;visible:picture.status!==Image.Ready}
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
