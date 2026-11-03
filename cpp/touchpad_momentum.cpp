// Kinetic scrolling for touchpads.
//
// GTK and Chromium keep a touchpad scroll coasting after the fingers lift; Qt does not,
// on Linux, so a list or article in Pheme stops dead the moment you lift. Compositors that
// slow touchpad scrolling down for those apps (Omarchy's Hyprland uses scroll_factor 0.4)
// leave Pheme feeling like it barely moves. This filter supplies the missing momentum.
//
// A touchpad scroll arrives as a gesture: ScrollBegin, ScrollUpdates, then ScrollEnd when
// the fingers lift. The filter holds back that ScrollEnd and continues the gesture with
// ScrollUpdates that slow down over time, then ends it. To Qt Quick's Flickable and to
// WebEngine alike it looks like the fingers kept moving, so both panes coast the same way.
// Mouse wheels send no phases and pass through untouched.

#include <QtCore/QCoreApplication>
#include <QtCore/QElapsedTimer>
#include <QtCore/QPointer>
#include <QtCore/QTimer>
#include <QtGui/QGuiApplication>
#include <QtGui/QPointingDevice>
#include <QtGui/QWheelEvent>
#include <QtGui/QWindow>

#include <cmath>
#include <deque>

namespace {

class TouchpadMomentum final : public QObject
{
public:
    explicit TouchpadMomentum(QObject *parent)
        : QObject(parent)
    {
        m_timer.setTimerType(Qt::PreciseTimer);
        m_timer.setInterval(8);
        QObject::connect(&m_timer, &QTimer::timeout, this, [this] { step(); });
    }

protected:
    bool eventFilter(QObject *object, QEvent *event) override
    {
        if (m_sending)
            return false;
        switch (event->type()) {
        case QEvent::Wheel:
            if (object->isWindowType())
                return wheel(static_cast<QWindow *>(object), static_cast<QWheelEvent *>(event));
            return false;
        case QEvent::MouseButtonPress:
        case QEvent::KeyPress:
        case QEvent::TouchBegin:
        case QEvent::Hide:
        case QEvent::FocusOut:
            if (object->isWindowType())
                stop();
            return false;
        default:
            return false;
        }
    }

private:
    // How far back the fingers' last movements count toward the lift-off velocity, as GTK.
    static constexpr qint64 HistoryMs = 150;
    // Fingers that rest this long before lifting have stopped the scroll themselves.
    static constexpr qint64 RestMs = 50;
    // Velocity decays as exp(-Friction * seconds), GTK's kinetic deceleration.
    static constexpr double Friction = 4.0;
    // In pixels per millisecond.
    static constexpr double MinVelocity = 0.02;
    static constexpr double MaxVelocity = 10.0;

    struct Sample {
        quint64 time;
        QPointF delta;
    };

    /// Returns whether the event was consumed.
    bool wheel(QWindow *window, QWheelEvent *event)
    {
        const Qt::ScrollPhase phase = event->phase();
        // A mouse wheel, or a platform that brings its own momentum (macOS).
        if (phase == Qt::NoScrollPhase || phase == Qt::ScrollMomentum) {
            stop();
            return false;
        }
        if (phase == Qt::ScrollBegin) {
            stop();
            m_history.clear();
            return false;
        }
        if (phase == Qt::ScrollUpdate) {
            if (m_timer.isActive())
                stop();
            if (!event->pixelDelta().isNull()) {
                m_history.push_back({event->timestamp(), QPointF(event->pixelDelta())});
                while (!m_history.empty() && event->timestamp() - m_history.front().time > HistoryMs)
                    m_history.pop_front();
                m_last = Template::from(window, event);
            }
            return false;
        }
        // ScrollEnd: the fingers lifted.
        const QPointF velocity = liftVelocity(event->timestamp());
        m_history.clear();
        if (std::hypot(velocity.x(), velocity.y()) < MinVelocity || !m_last.window)
            return false;
        m_velocity = velocity;
        m_carry = QPointF();
        m_clock.start();
        m_elapsed = 0;
        m_timer.start();
        return true;
    }

    QPointF liftVelocity(quint64 endTime) const
    {
        if (m_history.empty() || endTime - m_history.back().time > RestMs)
            return {};
        QPointF sum;
        for (const Sample &s : m_history)
            sum += s.delta;
        // The first sample's movement happened over the frame before it.
        const double spanMs = std::max<double>(16.0, double(m_history.back().time - m_history.front().time) + 8.0);
        QPointF v = sum / spanMs;
        const double speed = std::hypot(v.x(), v.y());
        if (speed > MaxVelocity)
            v *= MaxVelocity / speed;
        return v;
    }

    void step()
    {
        const qint64 now = m_clock.elapsed();
        const double dt = double(now - m_elapsed);
        m_elapsed = now;
        if (!m_last.window) {
            m_timer.stop();
            return;
        }
        // Distance covered over dt while the velocity decays: the exact integral, so
        // the coast is the same length whatever the timer's actual cadence.
        const double k = Friction / 1000.0;
        const double decay = std::exp(-k * dt);
        const QPointF distance = m_velocity * ((1.0 - decay) / k) + m_carry;
        m_velocity *= decay;
        const QPoint pixels(int(std::trunc(distance.x())), int(std::trunc(distance.y())));
        m_carry = distance - QPointF(pixels);
        if (!pixels.isNull())
            send(Qt::ScrollUpdate, pixels);
        if (std::hypot(m_velocity.x(), m_velocity.y()) < MinVelocity)
            stop();
    }

    /// Ends a coast in progress, closing the gesture it continued.
    void stop()
    {
        if (!m_timer.isActive())
            return;
        m_timer.stop();
        send(Qt::ScrollEnd, QPoint());
    }

    void send(Qt::ScrollPhase phase, QPoint pixels)
    {
        if (!m_last.window)
            return;
        QWheelEvent event(m_last.position, m_last.globalPosition, pixels, m_last.angleFor(pixels),
                          m_last.buttons, m_last.modifiers, phase, m_last.inverted, m_last.source,
                          m_last.device);
        event.setTimestamp(m_last.time + quint64(m_clock.elapsed()));
        m_sending = true;
        QCoreApplication::sendEvent(m_last.window, &event);
        m_sending = false;
    }

    /// The last real scroll event, which the coast's events copy.
    struct Template {
        QPointer<QWindow> window;
        QPointF position;
        QPointF globalPosition;
        Qt::MouseButtons buttons;
        Qt::KeyboardModifiers modifiers;
        bool inverted = false;
        Qt::MouseEventSource source = Qt::MouseEventNotSynthesized;
        const QPointingDevice *device = nullptr;
        quint64 time = 0;
        // Angle delta per pixel, as the platform reports them together.
        QPointF anglePerPixel{1.0, 1.0};

        static Template from(QWindow *window, const QWheelEvent *e)
        {
            Template t;
            t.window = window;
            t.position = e->position();
            t.globalPosition = e->globalPosition();
            t.buttons = e->buttons();
            t.modifiers = e->modifiers();
            t.inverted = e->inverted();
            t.source = e->source();
            t.device = e->pointingDevice();
            t.time = e->timestamp();
            const QPoint p = e->pixelDelta();
            const QPoint a = e->angleDelta();
            t.anglePerPixel = QPointF(p.x() ? double(a.x()) / p.x() : 1.0, p.y() ? double(a.y()) / p.y() : 1.0);
            return t;
        }

        QPoint angleFor(QPoint pixels) const
        {
            return QPoint(int(std::lround(pixels.x() * anglePerPixel.x())),
                          int(std::lround(pixels.y() * anglePerPixel.y())));
        }
    };

    QTimer m_timer;
    QElapsedTimer m_clock;
    qint64 m_elapsed = 0;
    std::deque<Sample> m_history;
    Template m_last;
    QPointF m_velocity;
    QPointF m_carry;
    bool m_sending = false;
};

}

void pheme_install_touchpad_momentum()
{
    qApp->installEventFilter(new TouchpadMomentum(qApp));
}
