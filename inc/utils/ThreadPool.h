#include <functional>
#include <future>
#include <mutex>
#include <queue>
#include <thread>
#include <utility>
#include <vector>
#include <spdlog/spdlog.h>
#include <ConcurrentCyclicQueue.h>

class ThreadPool
{
    public:
        ThreadPool(const size_t numThreads)
            : queue_(numThreads, 
                     std::chrono::milliseconds(timeout_)),
              terminate_(false),
              numOfThreads_(numThreads) {};

        ThreadPool(const ThreadPool &) = delete;
        ThreadPool(ThreadPool &&) = delete;

        ThreadPool & operator=(const ThreadPool &) = delete;
        ThreadPool & operator=(ThreadPool &&) = delete;

        static void worker(ConcurrentCyclicQueue<std::function<void()>> &queue,
                           size_t &id,
                           bool &terminate) {

            std::function<void()> funcPtr;

            while (!terminate) {
                if (true == queue.waitPop(funcPtr)) {
                    funcPtr();
                }
            }
        }

        void init() {

            for (size_t i = 0; i < numOfThreads_; ++i) {
                threads_.push_back(std::thread(worker,
                                               std::ref(queue_),
                                               std::ref(i),
                                               std::ref(terminate_)));
            }
        }

        void term() {

            terminate_ = true;
            
            for (size_t i = 0; i < threads_.size(); ++i) {
                if(threads_[i].joinable()) {
                    threads_[i].join();
                }
            }
        }

        // Submit a function to be executed asynchronously by the pool
        template<typename F, typename...Args>
        auto submit(F&& f, Args&&... args) -> std::future<decltype(f(args...))> {
            // Create a function with bounded parameters ready to execute
            std::function<decltype(f(args...))()> func = std::bind(std::forward<F>(f), 
                                                                   std::forward<Args>(args)...);

            // Encapsulate it into a shared ptr in order to be able to copy construct / assign 
            auto task_ptr = std::make_shared<std::packaged_task<decltype(f(args...))()>>(func);

            // Wrap packaged task into void function
            std::function<void()> wrapper_func = [task_ptr]() {
                (*task_ptr)(); 
            };

            if (true == queue_.isFull()) {
                spdlog::error("queue is full");
                return std::future<typename std::result_of<F(Args...)>::type>();
            }
            // Enqueue generic wrapper function
            if (false == queue_.push(wrapper_func)) {
                spdlog::error("failed to push to queue");
                return std::future<typename std::result_of<F(Args...)>::type>();
            }

            // Return future from promise
            return task_ptr->get_future();
        }

  private:

    ConcurrentCyclicQueue<std::function<void()>> queue_;
    bool terminate_;
    size_t numOfThreads_;
    std::vector<std::thread> threads_;

    static constexpr auto timeout_ = 100;
};
