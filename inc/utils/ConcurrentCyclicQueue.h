#ifndef __CONCURRENTCYCLICQUEUE_HPP__
#define __CONCURRENTCYCLICQUEUE_HPP__

#include <queue>
#include <mutex>
#include <condition_variable>

template<typename T>
class ConcurrentCyclicQueue final
{
public:
	explicit ConcurrentCyclicQueue(
		const size_t maxSize,
		const std::chrono::milliseconds milliseconds) :
			queueMaxSize_{maxSize},
			milliseconds_{milliseconds} {}

	ConcurrentCyclicQueue(const ConcurrentCyclicQueue&) = delete;
	ConcurrentCyclicQueue &operator=(const ConcurrentCyclicQueue&) = delete;

	virtual ~ConcurrentCyclicQueue() = default;

	bool push(T& data) noexcept	{
		std::unique_lock<std::mutex> uniqueLock(mutex_);
		if (queueMaxSize_ == queue_.size()) {
			queue_.pop();
		}

		queue_.push(std::move(data));
		uniqueLock.unlock();

		conditionVariable_.notify_one();
		
		return true;
	}

	bool isFull(void) const noexcept {
		std::unique_lock<std::mutex> uniqueLock(mutex_);

		return queueMaxSize_ == queue_.size();
	}

	bool isEmpty(void) const noexcept {
		std::unique_lock<std::mutex> uniqueLock(mutex_);

		return queue_.empty();
	}

	bool waitPop(T& popped_value) noexcept {
		std::unique_lock<std::mutex> uniqueLock(mutex_);
		if (queue_.empty()) {
			conditionVariable_.wait_for(
				uniqueLock,
				milliseconds_);

			if (queue_.empty())	{
				return false;
			}
		}

		popped_value = std::move(queue_.front());
		queue_.pop();

		return true;
	}

	size_t size(void) const noexcept {
		std::unique_lock<std::mutex> uniqueLock(mutex_);
		return queue_.size();
	}

	void clear(void) noexcept {
		std::unique_lock<std::mutex> uniqueLock(mutex_);
		while (!queue_.empty()) {
			queue_.pop();
		}
	}

private:
	static constexpr std::chrono::milliseconds DefaultPopQueueTimeout { 5U };

	size_t queueMaxSize_;
	mutable std::mutex mutex_;
	std::condition_variable conditionVariable_;
	std::chrono::milliseconds milliseconds_ { DefaultPopQueueTimeout };
	std::queue<T> queue_;
};
#endif // __CONCURRENTCYCLICQUEUE_HPP__