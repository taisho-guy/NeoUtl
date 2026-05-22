#include "lua_host.hpp"
#include <QTest>
#include <cmath>

using namespace AviQtl::Scripting;

class TestLuaHost : public QObject {
    Q_OBJECT

  private slots:
    void evaluateConstant() {
        QCOMPARE(LuaHost::evaluate("42", 0.0, 0, 0.0), 42.0);
        QCOMPARE(LuaHost::evaluate("3.14", 1.0, 0, 0.0), 3.14);
    }

    void evaluateTimeVariable() {
        QCOMPARE(LuaHost::evaluate("time", 5.0, 0, 0.0), 5.0);
        QCOMPARE(LuaHost::evaluate("t", 5.0, 0, 0.0), 5.0);
    }

    void evaluateIndexVariable() { QCOMPARE(LuaHost::evaluate("index", 0.0, 7, 0.0), 7.0); }

    void evaluateValueVariable() {
        QCOMPARE(LuaHost::evaluate("value", 0.0, 0, 3.14), 3.14);
        QCOMPARE(LuaHost::evaluate("v", 0.0, 0, 2.71), 2.71);
    }

    void evaluateMath() {
        double result = LuaHost::evaluate("sin(pi/2)", 0.0, 0, 0.0);
        QVERIFY(std::abs(result - 1.0) < 0.001);
    }

    void evaluateArithmetic() {
        QCOMPARE(LuaHost::evaluate("2 + 2 * 3", 0.0, 0, 0.0), 8.0);
        QCOMPARE(LuaHost::evaluate("(2 + 2) * 3", 0.0, 0, 0.0), 12.0);
    }

    void evaluateFallbackOnSyntaxError() {
        // Invalid Lua should return the fallback currentValue
        QCOMPARE(LuaHost::evaluate("this is not valid lua", 0.0, 0, 99.0), 99.0);
    }

    void evaluateFallbackOnRuntimeError() {
        // Division by zero in Lua returns inf, not an error
        double result = LuaHost::evaluate("1 / 0", 0.0, 0, 0.0);
        QVERIFY(std::isinf(result) || result > 1e9);
    }

    void evaluateCache() {
        // Same expression evaluated twice should hit the compiled cache
        double r1 = LuaHost::evaluate("m.cos(0)", 0.0, 0, 0.0);
        double r2 = LuaHost::evaluate("m.cos(0)", 0.0, 0, 0.0);
        QCOMPARE(r1, 1.0);
        QCOMPARE(r2, 1.0);
    }

    void evaluateComplexExpression() {
        double result = LuaHost::evaluate("value + sin(time) * index", 0.0, 2, 10.0);
        QCOMPARE(result, 10.0); // sin(0) = 0
    }

    // ── Extended: table / relational / math edge cases ──

    void evaluateTableLength() {
        // Lua tables can be manipulated; # returns length for 1-based numeric tables
        double result = LuaHost::evaluate("#{1,2,3}", 0.0, 0, 0.0);
        QCOMPARE(result, 3.0);
    }

    void evaluateMathFunctions() {
        double sqrtResult = LuaHost::evaluate("m.sqrt(16)", 0.0, 0, 0.0);
        QCOMPARE(sqrtResult, 4.0);

        double powResult = LuaHost::evaluate("m.pow(2, 10)", 0.0, 0, 0.0);
        QCOMPARE(powResult, 1024.0);

        double logResult = LuaHost::evaluate("m.log(m.exp(3))", 0.0, 0, 0.0);
        QVERIFY(std::abs(logResult - 3.0) < 0.001);
    }

    void evaluateStringConversion() {
        // tonumber converts string to number
        double result = LuaHost::evaluate("tonumber('42')", 0.0, 0, 0.0);
        QCOMPARE(result, 42.0);

        // invalid string should return fallback (0.0 here)
        double fallback = LuaHost::evaluate("tonumber('hello')", 1.0, 2, 0.0);
        QCOMPARE(fallback, 0.0);
    }

    void evaluateLargeNumbers() {
        double result = LuaHost::evaluate("1e12 + 1e12", 0.0, 0, 0.0);
        QCOMPARE(result, 2e12);
    }

    void evaluateZeroDivisionByLua() {
        // 1/0 in Lua produces +inf; we already tested that
        // 0/0 in Lua produces -nan(ind)
        double result = LuaHost::evaluate("0 / 0", 0.0, 0, 0.0);
        QVERIFY(std::isnan(result));
    }

    void evaluateAllVariablesCombined() {
        // time=2, index=3, value=10 -> 2*3 + 10 = 16
        double result = LuaHost::evaluate("time * index + value", 2.0, 3, 10.0);
        QCOMPARE(result, 16.0);
    }
};

QTEST_MAIN(TestLuaHost)
#include "test_lua_host.moc"
