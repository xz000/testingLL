using System.Collections;
using System.Collections.Generic;
using UnityEngine;
///using Photon;

public class StealthScript : MonoBehaviour
{
    public GameObject MyMarkicon;
    //public GameObject MyHealthbar;
    public GameObject MyColorMark;
    public SpriteRenderer BigSR;
    SpriteRenderer SmallSR;
    //public GameObject BarSR;
    public SpriteRenderer ColorSR;
    //public GameObject MyName;
    Color ColorBefore;
    public Color ColorChangeTo;
    float currenttime;
    float maxtime = 1;
    public float Speed = 0;
    bool WindWalkByTime = false;
    bool UCME = false;

    // Use this for initialization
    void Start()
    {
        SmallSR = MyMarkicon.GetComponent<SpriteRenderer>();
        //BarSR = MyHealthbar.GetComponent<SpriteRenderer>();
        //ColorSR = MyColorMark.GetComponent<SpriteRenderer>();
        //MyName = GetComponent<ShowMyName>();
    }

    public void PaintSmall(int paintNum)
    {
        ColorSR.sprite = Sender.theAvatars[paintNum];
    }

    void FixedUpdate()
    {
        if (!WindWalkByTime)
            return;
        currenttime += Time.fixedDeltaTime;
        if (currenttime >= maxtime)
            StealthEnd();
    }

    void Vanish()
    {
        if (GetComponent<MoveScript>().isme)
            SelfChange();
        else
        {
            BigSR.enabled = false;
            SmallSR.enabled = false;
            //BarSR.SetActive(false);
            ColorSR.enabled = false;
            GetComponent<ShowMyInfo>().DoNotShow();
            //MyName.SetActive(false);
        }
    }

    void Appear()
    {
        if (GetComponent<MoveScript>().isme)
            SelfChangeBack();
        else
        {
            BigSR.enabled = true;
            SmallSR.enabled = true;
            //BarSR.SetActive(true);
            ColorSR.enabled = true;
            GetComponent<ShowMyInfo>().DoShow();
            //MyName.SetActive(true);
        }
    }

    void SelfChange()
    {
        ColorBefore = BigSR.color;
        BigSR.color = ColorChangeTo;
        ColorSR.enabled = false;
    }

    void SelfChangeBack()
    {
        BigSR.color = ColorBefore;
        ColorSR.enabled = true;
    }

    public void StealthByTime(float time, bool DoLSDS)
    {
        currenttime = 0;
        maxtime = time;
        WindWalkByTime = true;
        if (DoLSDS)
            GetComponent<ColliderScript>().LSDSatAll();
        StealthStart();
    }

    public void StealthStart()
    {
        UCME = true;
        Vanish();
    }

    public void StealthEnd()
    {
        if (UCME == false)
            return;
        WindWalkByTime = false;
        GetComponent<ColliderScript>().DSWLatAll();
        Appear();
        Speed = 0;
        UCME = false;
    }

    private void OnMouseEnter()
    {
        SmallSR.color = Color.black;
    }

    private void OnMouseExit()
    {
        SmallSR.color = Color.white;
    }
}
