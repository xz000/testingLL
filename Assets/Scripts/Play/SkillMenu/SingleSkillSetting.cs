using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class SingleSkillSetting : MonoBehaviour
{

    public GameObject GoldObj;
    public Button MinusButton;
    public Button PlusButton;
    public Text LevelText;
    public Text LevelPriceText;
    public Toggle SkillToggle;
    public int SkillLevel = 0;
    public int BottomLevel = 0;
    public int TopLevel = 6;
    public int TopLevel00 = 6;
    public int TopLevel01;
    public int TopLevel02;
    public int TopLevel03;
    public int TopLevelJordan;
    public int LevelPrice = 5;// = { 11, 4, 5, 6, 7, 8, 0 };
    public int Level1Price = 11;
    public int Level2Price = 3;
    public int MultiInt = 1;
    private bool playsound = false;

    // Use this for initialization
    void Start()
    {
        GoldObj = GameObject.Find("TextGold");
        SkillToggle = GetComponent<Toggle>();
        SkillToggle.onValueChanged.AddListener(SkillToggleCtrl);
        LevelText = transform.Find("TextSkillLevel").GetComponent<Text>();
        LevelPriceText = transform.Find("TextPrice").GetComponent<Text>();
        MinusButton = transform.Find("ButtonMinus").GetComponent<Button>();
        MinusButton.onClick.AddListener(ClickMinusButton);
        PlusButton = transform.Find("ButtonPlus").GetComponent<Button>();
        PlusButton.onClick.AddListener(ClickPlusButton);
        TopLevel = 1;//Under Construction
        UpdatePrice();
        playsound = true;
    }

    public void SkillToggleCtrl(bool a)
    {
        if (a)
            TurnOn();
        else
            TurnOff();
    }

    void TurnOn()
    {
        if (SkillLevel == 0)
            ClickPlusButton();
    }

    void TurnOff()
    {
        while (SkillLevel > 0)
            ClickMinusButton();
    }

    public void ClickMinusButton()
    {
        SkillLevel -= 1;
        UpdatePrice();
        GoldObj.GetComponent<GoldScript>().GoldPoint += LevelPrice;
    }

    public void ClickPlusButton()
    {
        if (GoldObj.GetComponent<GoldScript>().GoldPoint < LevelPrice)
            return;
        GoldObj.GetComponent<GoldScript>().GoldPoint -= LevelPrice;
        SkillLevel += 1;
        if (!SkillToggle.isOn)
            SkillToggle.isOn = true;
        UpdatePrice();
    }

    void UpdatePrice()
    {
        if (SkillLevel == 0)
            LevelPrice = Level1Price;
        else if (SkillLevel == TopLevel)
            LevelPrice = 0;
        else
            LevelPrice = SkillLevel * MultiInt + Level2Price;
        LevelText.text = "Lv" + SkillLevel;
        LevelPriceText.text = "Price:" + LevelPrice;
        SetButtons();
        if (playsound)
            GameObject.Find("Main Camera").GetComponent<UISound>().PlayGoldSound();
    }

    void SetButtons()
    {
        SetMinusButton();
        SetPlusButton();
    }

    void SetMinusButton()
    {
        if (SkillLevel < 1 + BottomLevel)
        {
            MinusButton.gameObject.SetActive(false);
            if (SkillToggle.isOn)
                SkillToggle.isOn = false;
        }
        else
            MinusButton.gameObject.SetActive(true);
    }

    void SetPlusButton()
    {
        if (SkillLevel >= TopLevel)
            PlusButton.gameObject.SetActive(false);
        else
            PlusButton.gameObject.SetActive(true);
    }
}
