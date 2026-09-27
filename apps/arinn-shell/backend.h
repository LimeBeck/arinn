#pragma once
#include <QObject>
#include <QTimer>
#include <QVariantList>
class Backend : public QObject {
  Q_OBJECT
  Q_CLASSINFO("D-Bus Interface", "org.arinn.SpikeShell1")
  Q_PROPERTY(bool connected READ connected NOTIFY changed)
  Q_PROPERTY(bool available READ available NOTIFY changed)
  Q_PROPERTY(bool homeVisible READ homeVisible NOTIFY changed)
  Q_PROPERTY(QString returnWindow READ returnWindow NOTIFY changed)
  Q_PROPERTY(QVariantList items READ items NOTIFY changed)
  Q_PROPERTY(QString message READ message NOTIFY changed)
public:
  explicit Backend(QObject *parent = nullptr);
  bool connected() const { return m_connected; }
  bool available() const { return m_available; }
  bool homeVisible() const { return m_home; }
  QString returnWindow() const { return m_return; }
  QVariantList items() const { return m_items; }
  QString message() const { return m_message; }
  Q_INVOKABLE void back();
  Q_INVOKABLE void activate(const QString &id);
public slots:
  void refresh();
signals:
  void changed();
  void screenRequested(const QString &name);

private:
  bool m_connected = false, m_available = false, m_home = false,
       m_refreshing = false, m_dirty = false;
  quint64 m_connectionEpoch = 0, m_snapshotRevision = 0;
  QString m_generation, m_return, m_message;
  QVariantList m_items;
  QTimer m_timer;
};
