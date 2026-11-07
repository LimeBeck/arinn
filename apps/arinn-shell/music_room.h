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
  Q_PROPERTY(bool loading READ loading NOTIFY loadingChanged)
  // Some entry carries a date, so the default order is a date order.
  Q_PROPERTY(bool dated READ dated NOTIFY shelvesChanged)
  // Every source has answered its state once, or failed to: until then the
  // room does not claim to be the first use.
  Q_PROPERTY(bool settled READ settled NOTIFY sourcesChanged)
  // The room polls only while it is shown.
  Q_PROPERTY(bool active READ active WRITE setActive NOTIFY activeChanged)
  // Sources turned off in the Sources sheet, by provider id (spec §5.3.1,
  // #842): the room reads nothing from them and lists them last, `off`.
  Q_PROPERTY(QStringList disabled READ disabled WRITE setDisabled NOTIFY sourcesChanged)
  // Whether the choice can be stored now: the Sources sheet offers Turn off
  // and Turn on only then (Codex review of #851).
  Q_PROPERTY(bool sourcesSwitchable MEMBER m_switchable NOTIFY sourcesChanged)
  // Find over every source (MH-10, #801): result shelves Artists, Albums
  // and Tracks, each source's results in its own relevance order, sources
  // in turn; nothing merged.
  Q_PROPERTY(QVariantList searchShelves READ searchShelves NOTIFY searchChanged)
  // Names of the sources still answering the current search.
  Q_PROPERTY(QStringList searchPending READ searchPending NOTIFY searchChanged)
  Q_PROPERTY(QString searchQuery READ searchQuery NOTIFY searchChanged)
public:
  explicit MusicRoom(QObject *parent = nullptr);
  QObject *catalog() const { return m_catalog; }
  void setCatalog(QObject *catalog);
  QVariantList sources() const;
  QVariantList shelves() const { return m_shelves; }
  Q_INVOKABLE QString shelfTitle(const QString &id) const;
  bool connected() const;
  bool loading() const;
  bool dated() const { return m_dated; }
  bool settled() const;
  bool active() const { return m_timer.isActive(); }
  void setActive(bool active);
  QStringList disabled() const { return m_disabled; }
  void setDisabled(const QStringList &disabled);
  QVariantList searchShelves() const { return m_searchShelves; }
  QStringList searchPending() const;
  QString searchQuery() const { return m_searchQuery; }
  // Search every browsable source; an empty query clears the results.
  Q_INVOKABLE void search(const QString &query);
  // Each list's entries in its own order, one list after another in turn.
  static QVariantList alternate(const QList<QVariantList> &lists);
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
  // Turn a source off or on (#842): the shell stores the choice and sets
  // `disabled` from it.
  Q_INVOKABLE void setSourceEnabled(const QString &source, bool enabled) {
    emit sourceEnabledRequested(source, enabled);
  }
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
  void activeChanged();
  void searchChanged();
  void sourcesChanged();
  void shelvesChanged();
  // Cached presentation changed locally; consumers must not page providers.
  void presentationChanged();
  void loadingChanged();
  // Some page arrived or a source left: lists read again.
  void listChanged();
  void sourceEnabledRequested(const QString &source, bool enabled);

private slots:
  void follow();

private:
  struct Facet {
    QVariantList entries;
    QString next;
    quint64 revision = 0;
    bool busy = false, loaded = false, failed = false;
    // A new catalogue revision: the entries stay until the new first page
    // replaces them, so cards never leave and return (late sources rule).
    bool stale = false;
    // A continuation lease lasts 30 s (music.md): a later page, or a
    // continuation that fails, reads again from the first page into
    // `buffer` up to `resumeTo` entries, and the shown entries stay until it
    // has caught up (review of #812).
    QVariantList buffer;
    int resumeTo = 0;
    qint64 readAt = 0;
    // A failed first page is read again on a later state poll, at most every
    // few seconds (Codex review of #812).
    qint64 failedAt = 0;
    int retries = 0;
    QSet<QString> tokens;
  };
  struct Source {
    QString id, name, path, generation;
    bool ready = false;
    QVariantMap state;
    bool stateBusy = false, stateFailed = false, commandBusy = false,
         answered = false;
    QString stateError;
    // Normalized error code; translate when publishing, including language changes.
    QString commandError;
    quint64 catalogueRevision = 0, ticket = 0;
    std::map<QString, Facet> facets;
    QStringList pending;
  };
  void readState(const QString &id);
  void readNext(const QString &id);
  // `chained`: the read came from the source's chain of first pages, which
  // goes on once this facet's read (or its resume) ends.
  void readPage(const QString &id, const QString &facet, bool append,
                bool chained = false);
  void rebuild(bool presentationOnly = false);
  void startSearch(const QString &id);
  void readSearch(const QString &id);
  void continueSearch(const QString &id);
  void rebuildSearch();
  struct Search {
    std::map<QString, QVariantList> facets;
    QStringList pending;
    bool busy = false;
    // Replies to an earlier run of this source's search are dropped.
    quint64 ticket = 0;
    // A page refused while the provider's slots are busy is read again
    // once the others have answered, a few times (Codex review of #819).
    QStringList retry;
    std::map<QString, int> attempts;
  };
  std::map<QString, Search> m_search;
  QString m_searchQuery;
  // The browsable sources, by id, with the generation the search ran over.
  std::map<QString, QString> m_searchSources;
  std::map<QString, QString> searchSources() const;
  void reconcileSearch();
  quint64 m_searchTicket = 0;
  QVariantList m_searchShelves;
  void publishSources(bool presentationOnly = false);
  void publishLoading();
  QVariantList gather(const QString &shelf, const QString &source,
                      int limit) const;
  QPointer<QObject> m_catalog;
  QStringList m_order;
  // Turned-off sources (#842): left out of everything the room reads, and
  // listed last in `sources`. Whether each was this computer's is kept from
  // its last state, for its mark.
  QStringList m_disabled;
  bool m_switchable = true;
  struct Off {
    QString id, name;
    bool local = false;
  };
  QList<Off> m_off;
  std::map<QString, bool> m_local;
  // Identity reads in flight for turned-off sources whose mark is unknown.
  QSet<QString> m_identityReads;
  void readIdentity(const QString &id, const QString &path, const QString &generation);
  std::map<QString, Source> m_sources;
  QVariantList m_shelves, m_sourceRows;
  bool m_dated = false, m_loading = false;
  QTimer m_timer;
};
