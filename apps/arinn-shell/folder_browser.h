// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantList>

// Reads org.arinn.Folders1 (ADR-0071) for the folder picker (handoff PK-1…10).
// Only folder names: no files, no hidden entries. Calls are asynchronous; a
// newer request drops the reply of an older one. Paths are never logged.
class FolderBrowser : public QObject {
  Q_OBJECT
  // [{kind: home|mount|root, path, title}] in the daemon's order.
  Q_PROPERTY(QVariantList places READ places NOTIFY placesChanged)
  // The folder shown, an absolute path, or empty before the first open.
  Q_PROPERTY(QString path READ path NOTIFY listingChanged)
  // Subfolder names of `path`, sorted case-insensitively.
  Q_PROPERTY(QStringList folders READ folders NOTIFY listingChanged)
  // loading, ready, empty, denied, gone, unavailable
  Q_PROPERTY(QString state READ state NOTIFY listingChanged)
  // More names exist than were read (the listing stops at kMaxFolders).
  Q_PROPERTY(bool truncated READ truncated NOTIFY listingChanged)
public:
  explicit FolderBrowser(QObject *parent = nullptr);
  QVariantList places() const { return m_places; }
  QString path() const { return m_path; }
  QStringList folders() const { return m_folders; }
  QString state() const { return m_state; }
  bool truncated() const { return m_truncated; }
  // At most this many names are read for one folder.
  static constexpr int kMaxFolders = 2048;
  // The absolute path of `name` in `parent`, or of the parent of `path`.
  Q_INVOKABLE static QString child(const QString &parent, const QString &name);
  Q_INVOKABLE static QString parentOf(const QString &path);
public slots:
  void refreshPlaces();
  void open(const QString &path);
  void up();
signals:
  void placesChanged();
  void listingChanged();

private:
  void readPage(quint64 request, const QString &path, const QString &after,
                QStringList names);
  void finish(quint64 request, const QString &state, QStringList names,
              bool truncated);
  QVariantList m_places;
  QString m_path;
  QStringList m_folders;
  QString m_state = QStringLiteral("loading");
  bool m_truncated = false;
  quint64 m_request = 0;
  quint64 m_placesRequest = 0;
};
