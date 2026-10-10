// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#pragma once

#include <QObject>
#include <QVariantList>
#include <QVariantMap>

// The on-screen keyboard's layout and focus (spec §6.2, ADR-0103).
//
// The model owns the key rows and moves focus between keys; the QML panel
// only draws them and applies the result of activate() to its field. Rows,
// from top to bottom: an optional context row for the field (address or
// path), three letter rows of twelve (Russian JCUKEN or English QWERTY, or
// the digits and symbols layer), and the action row. Keys have flex widths
// as in the design handoff (KB-1…8).
//
// Left and right move within a row without wrapping. Up and down move to
// the key of the next row whose centre is nearest. Up from the top row
// leaves the keys, and the panel moves focus to its suggestions.
//
// The language and the last focused key are remembered per field while the
// shell runs; the typed text never is. Nothing is persisted.
class KeyboardModel : public QObject {
  Q_OBJECT
  // Memory key of the field, for example "browse".
  Q_PROPERTY(QString field READ field WRITE setField NOTIFY layoutChanged)
  // "address", "path" or empty: the context row above the letters.
  Q_PROPERTY(QString context READ context WRITE setContext NOTIFY layoutChanged)
  // Label of the main key, supplied by the field ("Open", "Save" …).
  Q_PROPERTY(QString submitLabel READ submitLabel WRITE setSubmitLabel NOTIFY
                 layoutChanged)
  // The interface language: the default letters of fields without context.
  Q_PROPERTY(QString uiLanguage READ uiLanguage WRITE setUiLanguage NOTIFY
                 layoutChanged)
  Q_PROPERTY(QString language READ language NOTIFY layoutChanged)
  Q_PROPERTY(bool symbols READ symbols NOTIFY layoutChanged)
  Q_PROPERTY(bool shift READ shift NOTIFY layoutChanged)
  Q_PROPERTY(QVariantList rows READ rows NOTIFY layoutChanged)
  Q_PROPERTY(int row READ row NOTIFY focusChanged)
  Q_PROPERTY(int column READ column NOTIFY focusChanged)
public:
  explicit KeyboardModel(QObject *parent = nullptr);

  QString field() const { return m_field; }
  void setField(const QString &field);
  QString context() const { return m_context; }
  void setContext(const QString &context);
  QString submitLabel() const { return m_submitLabel; }
  void setSubmitLabel(const QString &label);
  QString uiLanguage() const { return m_uiLanguage; }
  void setUiLanguage(const QString &language);
  QString language() const { return m_language; }
  bool symbols() const { return m_symbols; }
  bool shift() const { return m_shift; }
  QVariantList rows() const;
  int row() const { return m_row; }
  int column() const { return m_column; }

  // Prepares the panel for its field: the remembered language and key, or
  // the field's defaults on the first open.
  Q_INVOKABLE void open();
  // Remembers the language and the focused key for the next open.
  Q_INVOKABLE void close();
  // "left", "right", "up" or "down". Returns false when focus cannot move;
  // "up" from the top row returns false so the panel can leave the keys.
  Q_INVOKABLE bool move(const QString &direction);
  // Horizontal centre of the focused key, 0…1 of the row width.
  Q_INVOKABLE qreal focusX() const;
  // Focuses the key of the top row nearest to x (0…1), coming from above.
  Q_INVOKABLE void enterTop(qreal x);
  Q_INVOKABLE void focusKey(int row, int column);
  // Presses the focused key. Layer, language and Shift keys change the
  // model and return {action: "none"}. Otherwise the result is one of
  // {action: "insert", text}, "backspace", "left", "right" or "submit".
  Q_INVOKABLE QVariantMap activate();

  // Drops what this field remembered; the next open uses its defaults.
  Q_INVOKABLE void forget();
  // Tests start from a shell that has not remembered any field.
  static void forgetFields();

signals:
  void layoutChanged();
  void focusChanged();

private:
  struct Key {
    QString label, text, action, kind, icon;
    qreal width = 1;
  };
  QList<QList<Key>> keyRows() const;
  QString defaultLanguage() const;
  int topLetterRow() const { return m_context.isEmpty() ? 0 : 1; }
  void setFocus(int row, int column);
  static qreal centre(const QList<Key> &row, int column);
  static int nearest(const QList<Key> &row, qreal x);

  QString m_field, m_context, m_submitLabel, m_uiLanguage, m_language;
  bool m_symbols = false;
  bool m_shift = false;
  int m_row = 0;
  int m_column = 0;
};
