using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;
using UnityEngine.Networking;
using System.IO;
using System;
using System.Text;
using System.Runtime.Serialization.Formatters.Binary;
using Bond;
using Steamworks;

public class NetWriter : MonoBehaviour
{
    public Toggle RToggle;
    //public string netFileName;
    //public string nettxtPath;
    public float netFrameLength = 0.06f;
    float netCurrentLength = 0;
    public int PassedFrameNum = -1;
    public int ReceivedFrameNum = 0;
    public int LocalFrameNum = 1;
    public float LocalFrameLength = 0.06f;
    public float LocalCurrentLength = 0;
    public List<ClickData> L2S = new List<ClickData>();
    public static int channelID;
    //byte[] buffer2s;// = new byte[1024];
    public bool isstarted = false;
    //BinaryFormatter bf;
    //public bool Fstarted = false;
    public byte error;
    uint mn = 1;
    LoopList theLL;

    private void FixedUpdate()
    {
        /*if (!Fstarted)
            return;*/
        netCurrentLength += Time.fixedDeltaTime;
        while (netCurrentLength >= netFrameLength)
        {
            if (theLL.Numready(mn))
            {
                theLL.printhead();
                netCurrentLength -= netFrameLength;
                PassedFrameNum++;
                mn = 0;
            }
            else
            {
                Time.timeScale = 0;
                mn = 1;
                return;
            }
            //Debug.Log("pfn:" + PassedFrameNum);
        }
    }

    private void Update()
    {
        if (!isstarted)
            return;
        if ((LocalFrameNum - PassedFrameNum) < 5)
        {
            if (!RToggle.isOn)
                RToggle.isOn = true;
            LocalCurrentLength += Time.unscaledDeltaTime;
        }
        else
        {
            RToggle.isOn = false;
            LocalCurrentLength += Time.unscaledDeltaTime / 4;
        }
        while (LocalCurrentLength >= LocalFrameLength)
        {
            Data2S Fd2s = new Data2S();
            Fd2s.frameNum = LocalFrameNum;
            Fd2s.clientNum = Sender.clientNum;
            Fd2s.clickDatas = L2S;
            Bond.IO.Safe.OutputBuffer ob = new Bond.IO.Safe.OutputBuffer(64);
            Bond.Protocols.FastBinaryWriter<Bond.IO.Safe.OutputBuffer> bof = new Bond.Protocols.FastBinaryWriter<Bond.IO.Safe.OutputBuffer>(ob);
            Serialize.To(bof, Fd2s);
            ///NetworkTransport.Send(Sender.HSID, Sender.CNID, channelID, ob.Data.Array, ob.Data.Array.Length, out error);
            byte[] sendBytes = new byte[ob.Data.Array.Length + 1];
            sendBytes[0] = (byte)0;
            ob.Data.Array.CopyTo(sendBytes, 1);
            SteamNetworking.SendP2PPacket(Sender.TOmb, sendBytes, (uint)sendBytes.Length, EP2PSend.k_EP2PSendReliable);
            //
            int a = LocalFrameNum - PassedFrameNum - 1;
            theLL.addat(a, Sender.clientNum, L2S);
            if (Time.timeScale == 0 && theLL.Numready(3))
            {
                //isstarted = false;
                Time.timeScale = 1;
            }
            //
            L2S.Clear();
            //buffer2s = new byte[1024];
            LocalCurrentLength -= LocalFrameLength;
            //Debug.Log("Wey: " + LocalFrameNum);
            LocalFrameNum++;
        }
    }

    private void OnEnable()
    {
        Debug.Log("oe");
        theLL = new LoopList();
        theLL.init(GetComponent<ControllerScript>());
        PassedFrameNum = 0;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        isstarted = true;
        //Fstarted = true;
        Time.timeScale = 0;
    }

    private void OnDisable()
    {
        isstarted = false;
        //Fstarted = false;
        PassedFrameNum = -1;
        ReceivedFrameNum = 0;
        LocalFrameNum = 1;
        //theLL.stop();
    }

    Data2S bondbfd(byte[] bRC)
    {
        Bond.IO.Safe.InputBuffer ib = new Bond.IO.Safe.InputBuffer(bRC);
        Bond.Protocols.FastBinaryReader<Bond.IO.Safe.InputBuffer> fbr = new Bond.Protocols.FastBinaryReader<Bond.IO.Safe.InputBuffer>(ib);
        Data2S datarc = Deserialize<Data2S>.From(fbr);
        return datarc;
    }

    public void Eat(byte[] bRC)
    {
        Data2S datarc = bondbfd(bRC);//ttt
        ReceivedFrameNum = datarc.frameNum;
        int a = ReceivedFrameNum - PassedFrameNum - 1;
        theLL.addat(a, datarc.clientNum, datarc.clickDatas);
        if (Time.timeScale == 0 && theLL.Numready(3))
        {
            //isstarted = false;
            Time.timeScale = 1;
        }
    }
}
public class LoopList
{
    private int headnum;
    private int fullnum;
    private List<ClickData>[,] CDA2;
    private bool[,] bool3;
    private ControllerScript CTL;

    public void addat(int a, int b, List<ClickData> lcd)
    {
        if (a >= fullnum)
            jiabei();
        a += headnum;
        if (a >= fullnum)
            a -= fullnum;
        foreach (ClickData cd in lcd)
        {
            CDA2[a, b].Add(cd);
        }
        bool3[a, b] = true;
        bool3[a, 2] = bool3[a, 0] && bool3[a, 1];
    }

    public bool headready()
    {
        return bool3[headnum, 2];
    }

    public bool Numready(uint u)
    {
        bool a = true;
        for(int i= 0; i <= u; i++)
        {
            a = a && intready(headnum + i);
        }
        return a;
    }

    bool intready(int i)
    {
        if (i >= fullnum)
            i -= fullnum;
        return bool3[i, 2];
    }

    public void printhead()
    {
        CTL.powerrr(0, CDA2[headnum, 0]);
        CTL.powerrr(1, CDA2[headnum, 1]);
        for (int n = 0; n < 3; n++)
        {
            bool3[headnum, n] = false;
        }
        headnum++;
        if (headnum >= fullnum)
            headnum -= fullnum;
    }

    void jiabei()
    {
        int nextnum = fullnum * 2;
        int endingnum = fullnum + headnum;
        List<ClickData>[,] CDA2a = new List<ClickData>[nextnum, 2];
        bool[,] bool3a = new bool[nextnum, 3];
        for (int i = 0; i < nextnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                CDA2a[i, n] = new List<ClickData>();
                bool3a[i, n] = false;
            }
            bool3a[i, 2] = false;
        }
        for (int i = headnum; i < endingnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                if (i >= fullnum)
                {
                    int m = i - fullnum;
                    CDA2a[i, n] = new List<ClickData>(CDA2[m, n]);
                    bool3a[i, n] = bool3[m, n];
                    bool3a[i, 2] = bool3[m, 2];
                }
                else
                {
                    CDA2a[i, n] = new List<ClickData>(CDA2[i, n]);
                    bool3a[i, n] = bool3[i, n];
                    bool3a[i, 2] = bool3[i, 2];
                }
            }
        }
        CDA2 = CDA2a;
        bool3 = bool3a;
        fullnum = nextnum;
    }

    public void init(ControllerScript TCS)
    {
        CTL = TCS;
        CTL.createPCs(2);
        headnum = 0;
        fullnum = 32;
        CDA2 = new List<ClickData>[fullnum, 2];
        bool3 = new bool[fullnum, 3];
        for (int i = 0; i < fullnum; i++)
        {
            for (int n = 0; n < 2; n++)
            {
                CDA2[i, n] = new List<ClickData>();
                bool3[i, n] = false;
            }
            bool3[i, 2] = false;
        }
        GameObject.Find("Main Camera").GetComponent<CameraMove>().resetCam();
    }
}