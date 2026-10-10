// SPDX-FileCopyrightText: 2026 Arinn contributors
//
// SPDX-License-Identifier: GPL-3.0-or-later

#include "keyboard_model.h"

#include <QCoreApplication>
#include <QHash>
#include <QLocale>
#include <QQmlEngine>

#include <cmath>

namespace {
void registerKeyboardModel() {
  qmlRegisterType<KeyboardModel>("Arinn.Shell", 1, 0, "KeyboardModel");
}
Q_COREAPP_STARTUP_FUNCTION(registerKeyboardModel)

using Letters = QList<QStringList>;

const Letters &russian() {
  static const Letters rows{
      {QStringLiteral("й"), QStringLiteral("ц"), QStringLiteral("у"),
       QStringLiteral("к"), QStringLiteral("е"), QStringLiteral("н"),
       QStringLiteral("г"), QStringLiteral("ш"), QStringLiteral("щ"),
       QStringLiteral("з"), QStringLiteral("х"), QStringLiteral("ъ")},
      {QStringLiteral("ф"), QStringLiteral("ы"), QStringLiteral("в"),
       QStringLiteral("а"), QStringLiteral("п"), QStringLiteral("р"),
       QStringLiteral("о"), QStringLiteral("л"), QStringLiteral("д"),
       QStringLiteral("ж"), QStringLiteral("э"), QStringLiteral("ё")},
      {QStringLiteral("я"), QStringLiteral("ч"), QStringLiteral("с"),
       QStringLiteral("м"), QStringLiteral("и"), QStringLiteral("т"),
       QStringLiteral("ь"), QStringLiteral("б"), QStringLiteral("ю"),
       QStringLiteral("."), QStringLiteral(","), QStringLiteral("-")}};
  return rows;
}

const Letters &english() {
  static const Letters rows{
      {QStringLiteral("q"), QStringLiteral("w"), QStringLiteral("e"),
       QStringLiteral("r"), QStringLiteral("t"), QStringLiteral("y"),
       QStringLiteral("u"), QStringLiteral("i"), QStringLiteral("o"),
       QStringLiteral("p"), QStringLiteral("@"), QStringLiteral("_")},
      {QStringLiteral("a"), QStringLiteral("s"), QStringLiteral("d"),
       QStringLiteral("f"), QStringLiteral("g"), QStringLiteral("h"),
       QStringLiteral("j"), QStringLiteral("k"), QStringLiteral("l"),
       QStringLiteral(":"), QStringLiteral("'"), QStringLiteral("\"")},
      {QStringLiteral("z"), QStringLiteral("x"), QStringLiteral("c"),
       QStringLiteral("v"), QStringLiteral("b"), QStringLiteral("n"),
       QStringLiteral("m"), QStringLiteral(","), QStringLiteral("."),
       QStringLiteral("?"), QStringLiteral("!"), QStringLiteral("-")}};
  return rows;
}

const Letters &symbolLayer() {
  static const Letters rows{
      {QStringLiteral("1"), QStringLiteral("2"), QStringLiteral("3"),
       QStringLiteral("4"), QStringLiteral("5"), QStringLiteral("6"),
       QStringLiteral("7"), QStringLiteral("8"), QStringLiteral("9"),
       QStringLiteral("0"), QStringLiteral("-"), QStringLiteral("=")},
      {QStringLiteral("@"), QStringLiteral("#"), QStringLiteral("№"),
       QStringLiteral("%"), QStringLiteral("&"), QStringLiteral("*"),
       QStringLiteral("("), QStringLiteral(")"), QStringLiteral("_"),
       QStringLiteral("+"), QStringLiteral("\""), QStringLiteral("'")},
      {QStringLiteral(":"), QStringLiteral(";"), QStringLiteral(","),
       QStringLiteral("."), QStringLiteral("?"), QStringLiteral("!"),
       QStringLiteral("/"), QStringLiteral("\\"), QStringLiteral("|"),
       QStringLiteral("~"), QStringLiteral("$"), QStringLiteral("₽")}};
  return rows;
}

using Context = QList<QPair<QString, qreal>>;

const Context &addressContext() {
  static const Context keys{
      {QStringLiteral("https://"), 2}, {QStringLiteral("www."), 1.5},
      {QStringLiteral(".ru"), 1.5},    {QStringLiteral(".com"), 1.5},
      {QStringLiteral(".local"), 1.5}, {QStringLiteral("/"), 1},
      {QStringLiteral(":"), 1},        {QStringLiteral("."), 1},
      {QStringLiteral("-"), 1}};
  return keys;
}

const Context &pathContext() {
  static const Context keys{
      {QStringLiteral("/"), 1},         {QStringLiteral("~/"), 1},
      {QStringLiteral("/media/"), 1.8}, {QStringLiteral("/run/media/"), 2.2},
      {QStringLiteral("/mnt/"), 1.5},   {QStringLiteral(".."), 1},
      {QStringLiteral("-"), 1},         {QStringLiteral("_"), 1},
      {QStringLiteral("."), 1}};
  return keys;
}

struct Memory {
  QString language;
  int row = 0;
  int column = 0;
};

// Shell interaction state only: lost when the shell exits (§11).
QHash<QString, Memory> &memory() {
  static QHash<QString, Memory> fields;
  return fields;
}
} // namespace

KeyboardModel::KeyboardModel(QObject *parent)
    : QObject(parent), m_uiLanguage(QLocale().language() == QLocale::Russian
                                        ? QStringLiteral("ru")
                                        : QStringLiteral("en")) {
  m_language = defaultLanguage();
  m_row = topLetterRow() + 1;
  m_column = 4;
}

void KeyboardModel::setField(const QString &field) {
  if (field == m_field)
    return;
  m_field = field;
  emit layoutChanged();
}

void KeyboardModel::setContext(const QString &context) {
  if (context == m_context)
    return;
  m_context = context;
  emit layoutChanged();
  setFocus(m_row, m_column);
}

void KeyboardModel::setSubmitLabel(const QString &label) {
  if (label == m_submitLabel)
    return;
  m_submitLabel = label;
  emit layoutChanged();
}

void KeyboardModel::setUiLanguage(const QString &language) {
  if (language == m_uiLanguage)
    return;
  m_uiLanguage = language;
  emit layoutChanged();
}

QString KeyboardModel::defaultLanguage() const {
  // Addresses and paths are typed in Latin letters.
  return m_context.isEmpty() ? m_uiLanguage : QStringLiteral("en");
}

QList<QList<KeyboardModel::Key>> KeyboardModel::keyRows() const {
  QList<QList<Key>> rows;
  const Context *context =
      m_context == QStringLiteral("address") ? &addressContext()
      : m_context == QStringLiteral("path")  ? &pathContext()
                                             : nullptr;
  if (context) {
    QList<Key> row;
    for (const auto &[text, width] : *context)
      row.append({text,
                  text,
                  QStringLiteral("insert"),
                  QStringLiteral("context"),
                  {},
                  width});
    rows.append(row);
  }
  const Letters &letters = m_symbols                            ? symbolLayer()
                           : m_language == QStringLiteral("ru") ? russian()
                                                                : english();
  for (const auto &line : letters) {
    QList<Key> row;
    for (const auto &letter : line) {
      const auto shown = m_shift ? letter.toUpper() : letter;
      row.append({shown,
                  shown,
                  QStringLiteral("insert"),
                  QStringLiteral("char"),
                  {},
                  1});
    }
    rows.append(row);
  }
  rows.append({
      {{},
       {},
       QStringLiteral("shift"),
       QStringLiteral("action"),
       QStringLiteral("shift"),
       1},
      // The letters key names the alphabet it returns to, not the interface.
      {m_symbols ? (m_language == QStringLiteral("ru") ? QStringLiteral("АБВ")
                                                       : QStringLiteral("ABC"))
                 : QStringLiteral("123"),
       {},
       QStringLiteral("layer"),
       QStringLiteral("action"),
       {},
       1.5},
      {m_language == QStringLiteral("ru") ? tr("RU") : tr("EN"),
       {},
       QStringLiteral("language"),
       QStringLiteral("action"),
       QStringLiteral("language"),
       1.5},
      {tr("Space"),
       QStringLiteral(" "),
       QStringLiteral("insert"),
       QStringLiteral("action"),
       {},
       3},
      {{},
       {},
       QStringLiteral("left"),
       QStringLiteral("action"),
       QStringLiteral("left"),
       1},
      {{},
       {},
       QStringLiteral("right"),
       QStringLiteral("action"),
       QStringLiteral("right"),
       1},
      {{},
       {},
       QStringLiteral("backspace"),
       QStringLiteral("action"),
       QStringLiteral("backspace"),
       1.5},
      {m_submitLabel,
       {},
       QStringLiteral("submit"),
       QStringLiteral("primary"),
       {},
       1.5},
  });
  return rows;
}

QVariantList KeyboardModel::rows() const {
  QVariantList result;
  for (const auto &row : keyRows()) {
    QVariantList keys;
    for (const auto &key : row)
      keys.append(
          QVariantMap{{QStringLiteral("label"), key.label},
                      {QStringLiteral("action"), key.action},
                      {QStringLiteral("kind"), key.kind},
                      {QStringLiteral("icon"), key.icon},
                      {QStringLiteral("width"), key.width},
                      {QStringLiteral("on"),
                       key.action == QStringLiteral("shift") && m_shift}});
    result.append(QVariant(keys));
  }
  return result;
}

void KeyboardModel::setFocus(int row, int column) {
  const auto rows = keyRows();
  row = std::clamp(row, 0, int(rows.size()) - 1);
  column = std::clamp(column, 0, int(rows[row].size()) - 1);
  if (row == m_row && column == m_column)
    return;
  m_row = row;
  m_column = column;
  emit focusChanged();
}

void KeyboardModel::open() {
  const auto stored = memory().constFind(m_field);
  const bool remembered = !m_field.isEmpty() && stored != memory().constEnd();
  m_language = remembered ? stored->language : defaultLanguage();
  m_symbols = false;
  m_shift = false;
  emit layoutChanged();
  // First open: the fifth key of the middle letter row (KB-1).
  if (remembered)
    setFocus(stored->row, stored->column);
  else
    setFocus(topLetterRow() + 1, 4);
}

void KeyboardModel::close() {
  if (!m_field.isEmpty())
    memory().insert(m_field, {m_language, m_row, m_column});
}

void KeyboardModel::forgetFields() { memory().clear(); }

void KeyboardModel::forget() { memory().remove(m_field); }

qreal KeyboardModel::centre(const QList<Key> &row, int column) {
  qreal total = 0;
  qreal before = 0;
  for (int i = 0; i < row.size(); ++i) {
    if (i < column)
      before += row[i].width;
    total += row[i].width;
  }
  return (before + row[column].width / 2) / total;
}

int KeyboardModel::nearest(const QList<Key> &row, qreal x) {
  int best = 0;
  qreal distance = 2;
  for (int i = 0; i < row.size(); ++i) {
    const auto d = std::abs(centre(row, i) - x);
    if (d < distance) {
      distance = d;
      best = i;
    }
  }
  return best;
}

qreal KeyboardModel::focusX() const {
  return centre(keyRows()[m_row], m_column);
}

void KeyboardModel::enterTop(qreal x) { setFocus(0, nearest(keyRows()[0], x)); }

void KeyboardModel::focusKey(int row, int column) { setFocus(row, column); }

bool KeyboardModel::move(const QString &direction) {
  const auto rows = keyRows();
  const auto &row = rows[m_row];
  if (direction == QStringLiteral("left")) {
    if (m_column == 0)
      return false;
    setFocus(m_row, m_column - 1);
    return true;
  }
  if (direction == QStringLiteral("right")) {
    if (m_column >= row.size() - 1)
      return false;
    setFocus(m_row, m_column + 1);
    return true;
  }
  const int target = direction == QStringLiteral("up")     ? m_row - 1
                     : direction == QStringLiteral("down") ? m_row + 1
                                                           : -1;
  if (target < 0 || target >= rows.size())
    return false;
  setFocus(target, nearest(rows[target], centre(row, m_column)));
  return true;
}

QVariantMap KeyboardModel::activate() {
  const auto key = keyRows()[m_row][m_column];
  if (key.action == QStringLiteral("insert")) {
    // Shift is one-shot: it applies to the next character only.
    if (m_shift) {
      m_shift = false;
      emit layoutChanged();
    }
    return {{QStringLiteral("action"), QStringLiteral("insert")},
            {QStringLiteral("text"), key.text}};
  }
  if (key.action == QStringLiteral("shift")) {
    m_shift = !m_shift;
  } else if (key.action == QStringLiteral("layer")) {
    // Same grid: focus keeps its position.
    m_symbols = !m_symbols;
  } else if (key.action == QStringLiteral("language")) {
    m_language = m_language == QStringLiteral("ru") ? QStringLiteral("en")
                                                    : QStringLiteral("ru");
    m_symbols = false;
  } else {
    return {{QStringLiteral("action"), key.action}};
  }
  emit layoutChanged();
  return {{QStringLiteral("action"), QStringLiteral("none")}};
}
