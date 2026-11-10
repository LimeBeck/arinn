// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once

#include <QImage>
#include <QQuickPaintedItem>
#include <QString>

class VerificationQr : public QQuickPaintedItem {
  Q_OBJECT
  Q_PROPERTY(QString verificationUrl READ verificationUrl WRITE
                 setVerificationUrl NOTIFY verificationUrlChanged)
  Q_PROPERTY(bool valid READ valid NOTIFY verificationUrlChanged)

public:
  using QQuickPaintedItem::QQuickPaintedItem;
  QString verificationUrl() const { return m_url; }
  bool valid() const { return !m_image.isNull(); }
  void setVerificationUrl(const QString &value);
  void paint(QPainter *painter) override;

signals:
  void verificationUrlChanged();

private:
  QString m_url;
  QImage m_image;
};
