/* SPDX-FileCopyrightText: 2026 Arinn contributors
 * SPDX-License-Identifier: GPL-3.0-or-later */
#pragma once
#include <QHash>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QTimer>
#include <QVariantList>
#include <QVariantMap>
#include <map>

// The Music root as one room over every connected source (ADR-0142 MH-1,
// MH-5, MH-9, MH-10; #800). It reads the source list from the music catalogue,
// asks each source for its state and the first page of every facet a shelf
// needs, and gathers the shelves. Browsing never selects a source: the
// catalogue's own browsed source stays the one for a detail, the player and
// the queue.
class MusicRoom : public QObject {
  Q_OBJECT
  // The music catalogue whose `sources` (id, name, path, generation, ready)
  // this room follows. Any object with that property works.
  Q_PROPERTY(QObject *catalog READ catalog WRITE setCatalog NOTIFY catalogChanged)
  // The Sources sheet: {id, name, local, account, status, reason, actions,
  // prompt, busy} in the catalogue's order.
  Q_PROPERTY(QVariantList sources READ sources NOTIFY sourcesChanged)
  // The shelves that have entries: {id, title, kind, entries}. A shelf no
  // source can supply is absent, not empty.
  Q_PROPERTY(QVariantList shelves READ shelves NOTIFY shelvesChanged)
  // Some source offers a catalogue; without one the room is the first use.
  Q_PROPERTY(bool connected READ connected NOTIFY sourcesChanged)
  // A source has not answered its first pages yet.
  Q_PROPERTY(bool loading READ loading NOTIFY shelvesChanged)
  // Some entry carries a date, so the default order is a date order.
  Q_PROPERTY(bool dated READ dated NOTIFY shelvesChanged)
public:
  explicit MusicRoom(QObject *parent = nullptr);
  QObject *catalog() const { return m_catalog; }
  void setCatalog(QObject *catalog);
  QVariantList sources() const;
  QVariantList shelves() const { return m_shelves; }
  bool connected() const;
  bool loading() const;
  bool dated() const { return m_dated; }
  // The full list behind a shelf's All tile, over every page read so far:
  // every source, or one source (MH-10), in the shelf order or by title.
  Q_INVOKABLE QVariantList list(const QString &shelf,
                                const QString &source = QString(),
                                bool alphabetical = false) const;
  Q_INVOKABLE bool more(const QString &shelf,
                        const QString &source = QString()) const;
  Q_INVOKABLE void loadMore(const QString &shelf,
                            const QString &source = QString());
  // Connect, authorize, cancel, scan (Refresh) or disconnect, sent to one
  // source by its own state's capabilities (MH-5).
  Q_INVOKABLE void sourceCommand(const QString &source, const QString &action);
  Q_INVOKABLE void refresh();

  // MH-9: each provider's personal entries first, providers in Sources
  // order; then the ordinary entries, dated ones newest first, then the
  // undated ones one provider after another in turn, each in its own order.
  static QVariantList order(const QList<QVariantList> &personal,
                            const QList<QVariantList> &ordinary);
  // Late entries never move focus: while a shelf holds focus, the entries it
  // shows keep their places and new ones join at the end; entries that are
  // gone leave. Entries are matched by `key`.
  Q_INVOKABLE static QVariantList stable(const QVariantList &shown,
                                         const QVariantList &desired);
signals:
  void catalogChanged();
  void sourcesChanged();
  void shelvesChanged();
  // Some page arrived or a source left: lists read again.
  void listChanged();

private slots:
  void follow();

private:
  struct Facet {
    QVariantList entries;
    QString next;
    quint64 revision = 0;
    bool busy = false, loaded = false, failed = false;
    QSet<QString> tokens;
  };
  struct Source {
    QString id, name, path, generation;
    bool ready = false;
    QVariantMap state;
    bool stateBusy = false, stateFailed = false, commandBusy = false;
    QString stateError;
    quint64 catalogueRevision = 0, ticket = 0;
    std::map<QString, Facet> facets;
    QStringList pending;
  };
  void readState(const QString &id);
  void readNext(const QString &id);
  void readPage(const QString &id, const QString &facet, bool append);
  void rebuild();
  QVariantList gather(const QString &shelf, const QString &source,
                      int limit) const;
  QPointer<QObject> m_catalog;
  QStringList m_order;
  std::map<QString, Source> m_sources;
  QVariantList m_shelves;
  bool m_dated = false;
  QTimer m_timer;
};
