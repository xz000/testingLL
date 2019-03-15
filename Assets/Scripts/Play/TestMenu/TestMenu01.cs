using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using Steamworks;

public class TestMenu01 : MonoBehaviour
{
    //public Button backButton;               //返回按钮
    public GameObject roomMessagePanel;     //房间信息面板
    public GameObject previousButton;       //"上一页"按钮
    public GameObject nextButton;           //"下一页"按钮
    public Text pageMessage;                //房间页数文本控件
    public GameObject JoinOrCreateButton;      //创建房间面板
    public Text RoomName;

    ///private RoomInfo[] roomInfo;            //游戏大厅房间列表信息
    CallResult<LobbyMatchList_t> m_LobbyMatchListCallResult;
    private Lobby[] m_Lobbies;
    private int currentPageNumber;          //当前房间页
    private int maxPageNumber;              //最大房间页
    private int roomPerPage = 4;            //每页显示房间个数
    private GameObject[] roomMessage;       //游戏房间信息

    ///public GameObject Menu00;
    public GameObject Menu02;
    //public CanvasGroup RightGroup;
    public Toggle AutoCreateToggle;

    protected Callback<LobbyCreated_t> Callback_lobbyCreated;
    protected Callback<LobbyMatchList_t> Callback_lobbyList;
    protected Callback<LobbyEnter_t> Callback_lobbyEnter;
    protected Callback<LobbyDataUpdate_t> Callback_lobbyInfo;

    ///ulong current_lobbyID;
    List<CSteamID> lobbyIDS;

    void Start()
    {
        ///Object.DontDestroyOnLoad(gameObject);
        lobbyIDS = new List<CSteamID>();
        Callback_lobbyCreated = Callback<LobbyCreated_t>.Create(OnLobbyCreated);
        Callback_lobbyList = Callback<LobbyMatchList_t>.Create(OnGetLobbiesList);
        Callback_lobbyEnter = Callback<LobbyEnter_t>.Create(OnLobbyEntered);
        Callback_lobbyInfo = Callback<LobbyDataUpdate_t>.Create(OnGetLobbyInfo);
    }

    void OnLobbyCreated(LobbyCreated_t result)
    {
        if (result.m_eResult == EResult.k_EResultOK)
            Debug.Log("Lobby created -- SUCCESS!");
        else
            Debug.Log("Lobby created -- failure ...");
        string personalName = SteamFriends.GetPersonaName();
        string ntr = RoomName.text;
        if (ntr == "")
            ntr = personalName + "'s game";
        SteamMatchmaking.SetLobbyData((CSteamID)result.m_ulSteamIDLobby, "name", ntr);
    }

    void OnGetLobbiesList(LobbyMatchList_t result)
    {
        //Debug.Log("Found " + result.m_nLobbiesMatching + " lobbies!");
        for (int i = 0; i < result.m_nLobbiesMatching; i++)
        {
            CSteamID lobbyID = SteamMatchmaking.GetLobbyByIndex(i);
            lobbyIDS.Add(lobbyID);
            SteamMatchmaking.RequestLobbyData(lobbyID);
        }
    }

    void OnGetLobbyInfo(LobbyDataUpdate_t result)
    {
        ButtonControl();
    }

    void OnLobbyEntered(LobbyEnter_t result)
    {
        ///current_lobbyID = result.m_ulSteamIDLobby;
        Sender.roomid = (CSteamID)result.m_ulSteamIDLobby;
        if (result.m_EChatRoomEnterResponse == 1)
            SwitchToMenu02();
        else
            Debug.Log("Failed to join lobby.");
    }

    //当游戏大厅面板启用时调用，初始化信息
    void OnEnable()
    {
        m_LobbyMatchListCallResult = CallResult<LobbyMatchList_t>.Create(OnLobbyMatchList);
        m_LobbyMatchListCallResult.Set(SteamMatchmaking.RequestLobbyList());
        currentPageNumber = 1;              //初始化当前房间页
        maxPageNumber = 1;                  //初始化最大房间页	

        //获取房间信息面板
        RectTransform rectTransform = roomMessagePanel.GetComponent<RectTransform>();
        roomPerPage = rectTransform.childCount;     //获取房间信息面板的条目数

        //初始化每条房间信息条目
        roomMessage = new GameObject[roomPerPage];
        for (int i = 0; i < roomPerPage; i++)
        {
            roomMessage[i] = rectTransform.GetChild(i).gameObject;
            roomMessage[i].SetActive(false);            //禁用房间信息条目
        }
        ButtonControl();
        if (AutoCreateToggle.isOn)
            ClickJoinOrCreateButton();
    }

    public void Refresh()
    {
        m_LobbyMatchListCallResult.Set(SteamMatchmaking.RequestLobbyList());
    }

    private void OnLobbyMatchList(LobbyMatchList_t pCallback, bool bIOFailure)
    {
        if (bIOFailure)
        {
            var reason = SteamUtils.GetAPICallFailureReason(m_LobbyMatchListCallResult.Handle);
            Debug.LogError("OnLobbyMatchList encountered an IOFailure due to: " + reason);
            return;
        }
        if (pCallback.m_nLobbiesMatching == 0)
        {
            return;
        }

        m_Lobbies = new Lobby[pCallback.m_nLobbiesMatching];
        for (var i = 0; i < pCallback.m_nLobbiesMatching; ++i)
        {
            UpdateLobbyInfo(SteamMatchmaking.GetLobbyByIndex(i), ref m_Lobbies[i]);
        }
        RoomListUpdate();
    }

    private static void UpdateLobbyInfo(CSteamID steamIDLobby, ref Lobby outLobby)
    {
        outLobby.m_SteamID = steamIDLobby;
        outLobby.m_Owner = SteamMatchmaking.GetLobbyOwner(steamIDLobby);
        outLobby.m_Members = new LobbyMembers[SteamMatchmaking.GetNumLobbyMembers(steamIDLobby)];
        outLobby.m_MemberLimit = SteamMatchmaking.GetLobbyMemberLimit(steamIDLobby);

        var nDataCount = SteamMatchmaking.GetLobbyDataCount(steamIDLobby);
        outLobby.m_Data = new LobbyMetaData[nDataCount];
        for (var i = 0; i < nDataCount; ++i)
        {
            var lobby_data_ret = SteamMatchmaking.GetLobbyDataByIndex(steamIDLobby, i, out outLobby.m_Data[i].m_Key, Constants.k_nMaxLobbyKeyLength, out outLobby.m_Data[i].m_Value, Constants.k_cubChatMetadataMax);
            if (lobby_data_ret) continue;
            Debug.LogError("SteamMatchmaking.GetLobbyDataByIndex returned false.");
            continue;
        }
    }

    public void RoomListUpdate()
    {
        maxPageNumber = (m_Lobbies.Length - 1) / roomPerPage + 1;    //计算房间总页数
        if (currentPageNumber > maxPageNumber)      //如果当前页大于房间总页数时
            currentPageNumber = maxPageNumber;      //将当前房间页设为房间总页数
        pageMessage.text = currentPageNumber.ToString() + "/" + maxPageNumber.ToString();   //更新房间页数信息的显示
        ButtonControl();        //翻页按钮控制
        ShowRoomMessage();      //显示房间信息
    }

    //显示房间信息
    void ShowRoomMessage()
    {
        int start, end, i, j;
        start = (currentPageNumber - 1) * roomPerPage;          //计算需要显示房间信息的起始序号
        if (currentPageNumber * roomPerPage < m_Lobbies.Length)  //计算需要显示房间信息的末尾序号
            end = currentPageNumber * roomPerPage;
        else
            end = m_Lobbies.Length;

        //依次显示每条房间信息
        for (i = start, j = 0; i < end; i++, j++)
        {
            RectTransform rectTransform = roomMessage[j].GetComponent<RectTransform>();
            CSteamID LobbyID = m_Lobbies[i].m_SteamID;
            string roomName = SteamMatchmaking.GetLobbyData(LobbyID, "name"); //获取房间名称
            rectTransform.GetChild(0).GetComponent<Text>().text = (i + 1).ToString();   //显示房间序号
            rectTransform.GetChild(1).GetComponent<Text>().text = roomName;             //显示房间名称
            rectTransform.GetChild(2).GetComponent<Text>().text
                = m_Lobbies[i].m_Members.Length + "/" + m_Lobbies[i].m_MemberLimit;
            Button button = rectTransform.GetChild(3).GetComponent<Button>();               //获取"进入房间"按钮组件
            button.gameObject.SetActive(true);
            button.onClick.RemoveAllListeners();
            button.onClick.AddListener(delegate () {
                ClickJoinRoomButton(LobbyID);
            });
            roomMessage[j].SetActive(true); //启用房间信息条目
        }
        //禁用不显示的房间信息条目
        while (j < 4)
        {
            roomMessage[j++].SetActive(false);
        }
    }

    //翻页按钮控制函数
    void ButtonControl()
    {
        JoinOrCreateButton.SetActive(true);
        //如果当前页为1，禁用"上一页"按钮；否则，启用"上一页"按钮
        if (currentPageNumber == 1)
            previousButton.SetActive(false);
        else
            previousButton.SetActive(true);
        //如果当前页等于房间总页数，禁用"下一页"按钮；否则，启用"下一页"按钮
        if (currentPageNumber == maxPageNumber)
            nextButton.SetActive(false);
        else
            nextButton.SetActive(true);
    }

    //"创建房间"按钮事件处理函数，启用创建房间面板
    public void ClickJoinOrCreateButton()
    {
        SteamMatchmaking.CreateLobby(ELobbyType.k_ELobbyTypePublic, 2);
    }

    //"上一页"按钮事件处理函数
    public void ClickPreviousButton()
    {
        currentPageNumber--;        //当前房间页减一
        pageMessage.text = currentPageNumber.ToString() + "/" + maxPageNumber.ToString();   //更新房间页数显示
        ButtonControl();            //当前房间页更新，调动翻页控制函数
        ShowRoomMessage();          //当前房间页更新，重新显示房间信息
    }

    //"下一页"按钮事件处理函数
    public void ClickNextButton()
    {
        currentPageNumber++;        //当前房间页加一
        pageMessage.text = currentPageNumber.ToString() + "/" + maxPageNumber.ToString();   //更新房间页数显示
        ButtonControl();            //当前房间页更新，调动翻页控制函数
        ShowRoomMessage();          //当前房间页更新，重新显示房间信息
    }

    //"进入房间"按钮事件处理函数
    public void ClickJoinRoomButton(CSteamID id)
    {
        SteamMatchmaking.JoinLobby(id);
    }

    public void RefreshSelf()
    {
        gameObject.SetActive(false);
        gameObject.SetActive(true);
    }

    public void SwitchToMenu02()
    {
        //RightGroup.interactable = false;
        gameObject.SetActive(false);
        Menu02.SetActive(true);
    }
}

struct Lobby
{
    public CSteamID m_SteamID;
    public CSteamID m_Owner;
    public LobbyMembers[] m_Members;
    public int m_MemberLimit;
    public LobbyMetaData[] m_Data;
}

struct LobbyMetaData
{
    public string m_Key;
    public string m_Value;
}

struct LobbyMembers
{
    public CSteamID m_SteamID;
    public LobbyMetaData[] m_Data;
}
