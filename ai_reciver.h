#pragma once
#include "bits/stdc++.h"
#include <string>
#include <vector>
struct extra_body_st{
    std::string thinking_type;
};
struct message_st{
    std::string role;
    std::string content; 
};
struct question_st{
    std::string api_key;
    std::string base_url;
    std::string model;
    std::vector<message_st> message;
    bool stream;
    std::string reasoning_effort;
    extra_body_st extra_body;
};